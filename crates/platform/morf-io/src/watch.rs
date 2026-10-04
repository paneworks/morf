//! Filesystem changes, pushed as they happen, from one inotify instance.
//!
//! Every [`Watch`] in the process shares one inotify descriptor and one
//! thread. The thread sleeps in `poll(2)` on that descriptor and an eventfd
//! (rung only to stop it), so a process watching twenty files costs nothing
//! until one of them changes: no thread per file, no timeout, no sleep loop.
//! When something does change, the thread files it with the watches it
//! concerns and rings [`crate::wake_all`], so a loop waiting in its own poll
//! collects it at once.
//!
//! What a watch reports:
//!
//! - **A file** (or anything that is not a directory, including a path that
//!   does not exist yet): its directory is watched and only its own name is
//!   reported. `Created` when it appears where there was nothing, `Changed`
//!   when it is written or replaced (an editor's write-then-rename is a
//!   change, not a creation), `Deleted` and `Moved` when it goes. When its
//!   directory does not exist either, the nearest ancestor that does is
//!   watched, and the watch follows the directories down as they are made.
//! - **A directory**: its entries, with `name` the entry's path relative to
//!   the directory, and the directory itself going (`Deleted`/`Moved`, after
//!   which it is watched from its parent again, as a path that does not
//!   exist). `recursive` adds every directory below it, up to
//!   [`MAX_RECURSIVE_DIRS`], and those made later.
//!
//! Changes queue per watch and are coalesced by path until they are taken:
//! a burst of writes is one `Changed`, a file made and removed before anyone
//! looked is nothing, one removed and made again is a `Changed`. Memory is
//! bounded by the number of distinct paths waiting ([`MAX_PENDING`]); past
//! it, the watch is told its own path `Changed` instead, meaning "look
//! again", as it is when the kernel's own queue overflows.
//!
//! - **A sysfs attribute** (a file under `/sys`): inotify never fires
//!   there, so the attribute itself is polled beside the inotify descriptor
//!   and the kernel's `sysfs_notify` is a `Changed` (see `watch_sysfs`).
//!
//! The thread starts with the first watch and stops when the last one is
//! dropped.

mod service;

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::os::fd::OwnedFd;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rustix::fs::inotify::WatchFlags;

/// Directories one recursive watch adds at most.
pub const MAX_RECURSIVE_DIRS: usize = 4096;
/// Distinct paths one watch may have waiting before it is told to look again.
pub const MAX_PENDING: usize = 4096;

/// What happened to a path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ChangeKind {
    Changed,
    Created,
    Deleted,
    Moved,
}

impl ChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::Created => "created",
            Self::Deleted => "deleted",
            Self::Moved => "moved",
        }
    }

    /// What `self` followed by `next` amounts to, or `None` when nothing
    /// happened at all: made and gone again before anyone looked.
    fn then(self, next: Self) -> Option<Self> {
        use ChangeKind::*;
        match (self, next) {
            (Created, Changed | Created) => Some(Created),
            (Created, Deleted | Moved) => None,
            (Changed, Changed | Created) => Some(Changed),
            (Changed, gone @ (Deleted | Moved)) => Some(gone),
            (Deleted | Moved, Created | Changed) => Some(Changed),
            (Deleted | Moved, gone @ (Deleted | Moved)) => Some(gone),
        }
    }
}

/// One change to one path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FsChange {
    /// The path that changed.
    pub path: PathBuf,
    /// Relative to a watched directory for its entries; otherwise the file
    /// name of the watched path.
    pub name: PathBuf,
    pub kind: ChangeKind,
}

/// How to watch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WatchOptions {
    /// A directory's subdirectories too, and those made later.
    pub recursive: bool,
}

/// Changes waiting for a watch, one entry per path.
#[derive(Default)]
struct Pending {
    order: VecDeque<PathBuf>,
    changes: HashMap<PathBuf, (PathBuf, ChangeKind)>,
    overflowed: bool,
}

impl Pending {
    fn push(&mut self, change: FsChange) {
        if let Some((_, kind)) = self.changes.get_mut(&change.path) {
            match kind.then(change.kind) {
                Some(next) => *kind = next,
                None => {
                    self.changes.remove(&change.path);
                }
            }
            return;
        }
        if self.changes.len() >= MAX_PENDING {
            self.overflowed = true;
            return;
        }
        self.order.push_back(change.path.clone());
        self.changes.insert(change.path, (change.name, change.kind));
    }

    fn pop(&mut self, target: &Path) -> Option<FsChange> {
        while let Some(path) = self.order.pop_front() {
            if let Some((name, kind)) = self.changes.remove(&path) {
                return Some(FsChange { path, name, kind });
            }
        }
        if std::mem::take(&mut self.overflowed) {
            return Some(FsChange {
                path: target.to_path_buf(),
                name: file_name(target),
                kind: ChangeKind::Changed,
            });
        }
        None
    }

    fn is_empty(&self) -> bool {
        self.changes.is_empty() && !self.overflowed
    }
}

struct Subscription {
    target: PathBuf,
    recursive: bool,
    /// Whether the target was there when last seen.
    exists: bool,
    /// The directories watched for it, by watch descriptor.
    armed: HashMap<i32, PathBuf>,
    pending: Pending,
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    subscriptions: HashMap<u64, Subscription>,
    /// Which subscriptions use each watch descriptor.
    users: HashMap<i32, HashSet<u64>>,
}

struct Service {
    inotify: OwnedFd,
    wake: OwnedFd,
    /// The sysfs attributes watched, by subscription, polled for `POLLPRI`.
    attributes: Mutex<Vec<(u64, Arc<OwnedFd>)>>,
    stop: AtomicBool,
    inner: Mutex<Inner>,
    ready: Condvar,
    thread: Mutex<Option<JoinHandle<()>>>,
}

static SERVICE: Mutex<Option<Arc<Service>>> = Mutex::new(None);
static THREADS: AtomicUsize = AtomicUsize::new(0);

/// Watcher threads alive in this process: at most one, however many
/// watches there are.
pub fn watcher_threads() -> usize {
    THREADS.load(Ordering::Acquire)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

const MASK: WatchFlags = WatchFlags::CREATE
    .union(WatchFlags::DELETE)
    .union(WatchFlags::MODIFY)
    .union(WatchFlags::CLOSE_WRITE)
    .union(WatchFlags::ATTRIB)
    .union(WatchFlags::MOVED_FROM)
    .union(WatchFlags::MOVED_TO)
    .union(WatchFlags::DELETE_SELF)
    .union(WatchFlags::MOVE_SELF)
    .union(WatchFlags::ONLYDIR);

fn file_name(path: &Path) -> PathBuf {
    path.file_name().map(PathBuf::from).unwrap_or_default()
}

/// The path with `.` and `..` resolved by name, not by following links: a
/// watched path that does not exist cannot be canonicalised.
fn normalise(path: &Path) -> io::Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    Ok(out)
}

/// One watched path. Dropping it stops watching.
pub struct Watch {
    id: u64,
    target: PathBuf,
    service: Arc<Service>,
}

impl Watch {
    /// Starts watching `path`, which need not exist.
    pub fn new(path: impl AsRef<Path>, options: WatchOptions) -> io::Result<Self> {
        let target = normalise(path.as_ref())?;
        let mut registry = lock(&SERVICE);
        let service = match registry.as_ref() {
            Some(service) => Arc::clone(service),
            None => {
                let service = Service::start()?;
                *registry = Some(Arc::clone(&service));
                service
            }
        };
        match service.subscribe(target.clone(), options) {
            Ok(id) => Ok(Self {
                id,
                target,
                service,
            }),
            Err(error) => {
                let idle = lock(&service.inner).subscriptions.is_empty();
                if idle {
                    registry.take();
                    drop(registry);
                    service.shut_down();
                }
                Err(error)
            }
        }
    }

    /// The watched path, absolute.
    pub fn path(&self) -> &Path {
        &self.target
    }

    /// Everything that changed since the last look, coalesced by path.
    pub fn drain(&self) -> Vec<FsChange> {
        let mut inner = lock(&self.service.inner);
        let Some(subscription) = inner.subscriptions.get_mut(&self.id) else {
            return Vec::new();
        };
        let mut changes = Vec::new();
        while let Some(change) = subscription.pending.pop(&self.target) {
            changes.push(change);
        }
        changes
    }

    /// Whether anything is waiting.
    pub fn has_pending(&self) -> bool {
        lock(&self.service.inner)
            .subscriptions
            .get(&self.id)
            .is_some_and(|subscription| !subscription.pending.is_empty())
    }

    /// The next change, waiting up to `timeout` for one.
    pub fn next_timeout(&self, timeout: Duration) -> Option<FsChange> {
        let deadline = Instant::now() + timeout;
        let mut inner = lock(&self.service.inner);
        loop {
            let subscription = inner.subscriptions.get_mut(&self.id)?;
            if let Some(change) = subscription.pending.pop(&self.target) {
                return Some(change);
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            inner = self
                .service
                .ready
                .wait_timeout(inner, deadline - now)
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
    }
}

/// Waits until one of `watches` has something, or `timeout` passes (never,
/// given `None`). True when something is waiting.
///
/// For a thread that follows several paths at once and has nothing else to
/// do: it sleeps on the watcher's own condition variable, so it wakes only
/// when one of them has news.
pub fn wait_any(watches: &[Watch], timeout: Option<Duration>) -> bool {
    let Some(first) = watches.first() else {
        return false;
    };
    let service = &first.service;
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    let mut inner = lock(&service.inner);
    loop {
        let waiting = watches.iter().any(|watch| {
            Arc::ptr_eq(&watch.service, service)
                && inner
                    .subscriptions
                    .get(&watch.id)
                    .is_some_and(|subscription| !subscription.pending.is_empty())
        });
        if waiting {
            return true;
        }
        inner = match deadline {
            None => service
                .ready
                .wait(inner)
                .unwrap_or_else(|error| error.into_inner()),
            Some(deadline) => {
                let now = Instant::now();
                if now >= deadline {
                    return false;
                }
                service
                    .ready
                    .wait_timeout(inner, deadline - now)
                    .unwrap_or_else(|error| error.into_inner())
                    .0
            }
        };
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let mut registry = lock(&SERVICE);
        let idle = {
            let mut inner = lock(&self.service.inner);
            self.service.forget(&mut inner, self.id);
            inner.subscriptions.is_empty()
        };
        let current = registry
            .as_ref()
            .is_some_and(|service| Arc::ptr_eq(service, &self.service));
        if idle && current {
            registry.take();
            drop(registry);
            self.service.shut_down();
        }
    }
}

impl std::fmt::Debug for Watch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Watch")
            .field("path", &self.target)
            .finish()
    }
}
