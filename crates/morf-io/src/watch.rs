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

use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustix::event::{EventfdFlags, PollFd, PollFlags, eventfd, poll};
use rustix::fs::inotify::{self, CreateFlags, ReadFlags, WatchFlags};
use rustix::io::Errno;

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

impl Service {
    fn start() -> io::Result<Arc<Self>> {
        let service = Arc::new(Self {
            inotify: inotify::init(CreateFlags::CLOEXEC | CreateFlags::NONBLOCK)?,
            wake: eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)?,
            attributes: Mutex::new(Vec::new()),
            stop: AtomicBool::new(false),
            inner: Mutex::new(Inner::default()),
            ready: Condvar::new(),
            thread: Mutex::new(None),
        });
        let worker = Arc::clone(&service);
        THREADS.fetch_add(1, Ordering::AcqRel);
        let join = thread::Builder::new()
            .name("morf-fs-watch".to_owned())
            .spawn(move || {
                worker.run();
                THREADS.fetch_sub(1, Ordering::AcqRel);
            })
            .inspect_err(|_| {
                THREADS.fetch_sub(1, Ordering::AcqRel);
            })?;
        *lock(&service.thread) = Some(join);
        Ok(service)
    }

    fn run(&self) {
        let mut buffer = [MaybeUninit::<u8>::uninit(); 16 * 1024];
        let mut raw = Vec::new();
        loop {
            // The attributes as they are now: a watch added or dropped rings
            // `wake`, and the next turn polls the new set.
            let attributes = lock(&self.attributes).clone();
            let mut fds = vec![
                PollFd::new(&self.inotify, PollFlags::IN),
                PollFd::new(&self.wake, PollFlags::IN),
            ];
            for (_, fd) in &attributes {
                fds.push(PollFd::new(&**fd, PollFlags::PRI | PollFlags::ERR));
            }
            match poll(&mut fds, None) {
                Ok(_) => {}
                Err(Errno::INTR) => continue,
                Err(_) => return,
            }
            let woken = !fds[1].revents().is_empty();
            let changed = attributes
                .iter()
                .zip(&fds[2..])
                .filter(|(_, fd)| !fd.revents().is_empty())
                .map(|((id, fd), _)| (*id, Arc::clone(fd)))
                .collect::<Vec<_>>();
            drop(fds);
            if self.stop.load(Ordering::Acquire) {
                return;
            }
            if woken {
                let mut count = [0u8; 8];
                let _ = rustix::io::read(&self.wake, &mut count);
            }
            if !changed.is_empty() {
                let mut inner = lock(&self.inner);
                for (id, fd) in &changed {
                    crate::watch_sysfs::rearm(fd);
                    if let Some(subscription) = inner.subscriptions.get_mut(id) {
                        subscription.pending.push(FsChange {
                            path: subscription.target.clone(),
                            name: file_name(&subscription.target),
                            kind: ChangeKind::Changed,
                        });
                    }
                }
                drop(inner);
                self.ready.notify_all();
                crate::wake_all();
            }
            raw.clear();
            let mut reader = inotify::Reader::new(&self.inotify, &mut buffer);
            loop {
                match reader.next() {
                    Ok(event) => raw.push((
                        event.wd(),
                        event.events(),
                        event
                            .file_name()
                            .map(|name| PathBuf::from(OsStr::from_bytes(name.to_bytes()))),
                    )),
                    Err(Errno::INTR) => {}
                    Err(_) => break,
                }
            }
            if raw.is_empty() {
                continue;
            }
            let queued = self.dispatch(&raw);
            if queued {
                self.ready.notify_all();
                crate::wake_all();
            }
        }
    }

    /// Files a batch of kernel events with the subscriptions they concern.
    /// True when anything was queued.
    fn dispatch(&self, events: &[(i32, ReadFlags, Option<PathBuf>)]) -> bool {
        let mut inner = lock(&self.inner);
        let mut rearm = HashSet::new();
        let mut queued = false;
        for (wd, flags, name) in events {
            if flags.contains(ReadFlags::QUEUE_OVERFLOW) {
                // The kernel lost events: everyone looks again.
                for subscription in inner.subscriptions.values_mut() {
                    subscription.pending.overflowed = true;
                    queued = true;
                }
                continue;
            }
            let Some(users) = inner.users.get(wd).cloned() else {
                continue;
            };
            if flags.contains(ReadFlags::IGNORED) {
                // The directory is gone (or the watch was removed): whoever
                // relied on it finds its way again.
                inner.users.remove(wd);
                for id in users {
                    if let Some(subscription) = inner.subscriptions.get_mut(&id) {
                        subscription.armed.remove(wd);
                        rearm.insert(id);
                    }
                }
                continue;
            }
            for id in users {
                let Some(subscription) = inner.subscriptions.get_mut(&id) else {
                    continue;
                };
                let Some(dir) = subscription.armed.get(wd).cloned() else {
                    continue;
                };
                let (queue, again) = subscription.take(&dir, *flags, name.as_deref());
                queued |= queue;
                if again {
                    rearm.insert(id);
                }
            }
        }
        for id in rearm {
            queued |= self.arm(&mut inner, id).unwrap_or(false);
        }
        queued
    }

    /// Watches what subscription `id` needs now, letting go of what it no
    /// longer does. True when that turned up a change to report (the target
    /// appeared or vanished while it was not watched closely enough to see).
    fn arm(&self, inner: &mut Inner, id: u64) -> io::Result<bool> {
        let Some(subscription) = inner.subscriptions.get(&id) else {
            return Ok(false);
        };
        let target = subscription.target.clone();
        let recursive = subscription.recursive;
        let mut dirs = Vec::new();
        if target.is_dir() {
            dirs.push(target.clone());
            if recursive {
                let mut index = 0;
                while index < dirs.len() && dirs.len() < MAX_RECURSIVE_DIRS {
                    if let Ok(entries) = fs::read_dir(&dirs[index]) {
                        for entry in entries.flatten() {
                            if dirs.len() >= MAX_RECURSIVE_DIRS {
                                break;
                            }
                            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                                dirs.push(entry.path());
                            }
                        }
                    }
                    index += 1;
                }
            }
        } else {
            let mut ancestor = target.parent();
            while let Some(dir) = ancestor {
                if dir.is_dir() {
                    dirs.push(dir.to_path_buf());
                    break;
                }
                ancestor = dir.parent();
            }
        }
        let mut armed = HashMap::new();
        let mut first_error = None;
        for (index, dir) in dirs.into_iter().enumerate() {
            match inotify::add_watch(&self.inotify, &dir, MASK) {
                Ok(wd) => {
                    armed.insert(wd, dir);
                }
                // Only the first directory is essential; one below it that
                // vanished meanwhile or ran into the kernel's limit is left.
                Err(error) if index == 0 => first_error = Some(io::Error::from(error)),
                Err(_) => {}
            }
        }
        let subscription = inner.subscriptions.get_mut(&id).expect("looked up above");
        let old = std::mem::replace(&mut subscription.armed, armed);
        let exists = target.exists();
        let mut queued = false;
        if exists != subscription.exists {
            subscription.exists = exists;
            subscription.pending.push(FsChange {
                path: target.clone(),
                name: file_name(&target),
                kind: if exists {
                    ChangeKind::Created
                } else {
                    ChangeKind::Deleted
                },
            });
            queued = true;
        }
        let new = subscription.armed.keys().copied().collect::<Vec<_>>();
        for wd in new {
            inner.users.entry(wd).or_default().insert(id);
        }
        let now = &inner.subscriptions[&id].armed;
        let released = old
            .into_keys()
            .filter(|wd| !now.contains_key(wd))
            .collect::<Vec<_>>();
        for wd in released {
            self.release(inner, wd, id);
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(queued),
        }
    }

    /// Subscription `id` no longer needs `wd`; the kernel's watch goes when
    /// nobody does.
    fn release(&self, inner: &mut Inner, wd: i32, id: u64) {
        let Some(users) = inner.users.get_mut(&wd) else {
            return;
        };
        users.remove(&id);
        if users.is_empty() {
            inner.users.remove(&wd);
            let _ = inotify::remove_watch(&self.inotify, wd);
        }
    }

    fn subscribe(&self, target: PathBuf, options: WatchOptions) -> io::Result<u64> {
        let mut inner = lock(&self.inner);
        let id = inner.next_id;
        inner.next_id += 1;
        inner.subscriptions.insert(
            id,
            Subscription {
                exists: target.exists(),
                target,
                recursive: options.recursive,
                armed: HashMap::new(),
                pending: Pending::default(),
            },
        );
        let target = inner.subscriptions[&id].target.clone();
        match crate::watch_sysfs::open_attribute(&target) {
            Ok(Some(fd)) => {
                lock(&self.attributes).push((id, Arc::new(fd)));
                self.ring();
                return Ok(id);
            }
            Ok(None) => {}
            Err(error) => {
                self.forget(&mut inner, id);
                return Err(error);
            }
        }
        if let Err(error) = self.arm(&mut inner, id) {
            self.forget(&mut inner, id);
            return Err(error);
        }
        Ok(id)
    }

    /// Wakes the thread to poll its set anew.
    fn ring(&self) {
        let _ = rustix::io::write(&self.wake, &1u64.to_ne_bytes());
    }

    fn forget(&self, inner: &mut Inner, id: u64) {
        let mut attributes = lock(&self.attributes);
        let before = attributes.len();
        attributes.retain(|(held, _)| *held != id);
        if attributes.len() != before {
            drop(attributes);
            self.ring();
        }
        if let Some(subscription) = inner.subscriptions.remove(&id) {
            for wd in subscription.armed.into_keys() {
                self.release(inner, wd, id);
            }
        }
    }

    /// Stops the thread and waits for it. Called with the service already
    /// out of [`SERVICE`], so nobody new can find it.
    fn shut_down(&self) {
        self.stop.store(true, Ordering::Release);
        let _ = rustix::io::write(&self.wake, &1u64.to_ne_bytes());
        let join = lock(&self.thread).take();
        if let Some(join) = join
            && join.thread().id() != thread::current().id()
        {
            let _ = join.join();
        }
    }
}

impl Subscription {
    /// One kernel event on directory `dir`. Returns whether a change was
    /// queued and whether the watches must be worked out again.
    fn take(&mut self, dir: &Path, flags: ReadFlags, name: Option<&Path>) -> (bool, bool) {
        let appeared = flags.intersects(ReadFlags::CREATE | ReadFlags::MOVED_TO);
        let Some(name) = name else {
            // The watched directory itself.
            if flags.intersects(ReadFlags::DELETE_SELF | ReadFlags::MOVE_SELF) {
                let mut queued = false;
                if dir == self.target {
                    self.exists = false;
                    self.pending.push(FsChange {
                        path: self.target.clone(),
                        name: file_name(&self.target),
                        kind: if flags.contains(ReadFlags::DELETE_SELF) {
                            ChangeKind::Deleted
                        } else {
                            ChangeKind::Moved
                        },
                    });
                    queued = true;
                }
                return (queued, true);
            }
            return (false, false);
        };
        let path = dir.join(name);
        if path == self.target {
            let kind = if appeared {
                let kind = if self.exists {
                    ChangeKind::Changed
                } else {
                    ChangeKind::Created
                };
                self.exists = true;
                kind
            } else if flags.contains(ReadFlags::DELETE) {
                self.exists = false;
                ChangeKind::Deleted
            } else if flags.contains(ReadFlags::MOVED_FROM) {
                self.exists = false;
                ChangeKind::Moved
            } else {
                ChangeKind::Changed
            };
            self.pending.push(FsChange {
                path,
                name: file_name(&self.target),
                kind,
            });
            // A directory made where the target is: watch its entries now.
            let again = appeared && flags.contains(ReadFlags::ISDIR);
            return (true, again);
        }
        if self.target.starts_with(&path) {
            // A directory on the way down to the target.
            return (false, appeared && flags.contains(ReadFlags::ISDIR));
        }
        let Ok(relative) = path.strip_prefix(&self.target) else {
            return (false, false);
        };
        let kind = if appeared {
            ChangeKind::Created
        } else if flags.contains(ReadFlags::DELETE) {
            ChangeKind::Deleted
        } else if flags.contains(ReadFlags::MOVED_FROM) {
            ChangeKind::Moved
        } else {
            ChangeKind::Changed
        };
        let name = relative.to_path_buf();
        self.pending.push(FsChange { path, name, kind });
        let again = self.recursive
            && flags.contains(ReadFlags::ISDIR)
            && flags.intersects(ReadFlags::CREATE | ReadFlags::MOVED_TO | ReadFlags::MOVED_FROM);
        (true, again)
    }
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
