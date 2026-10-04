//! The watcher thread: one inotify instance, its watches, and how kernel
//! events become queued changes for each subscription.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self};

use rustix::event::{EventfdFlags, PollFd, PollFlags, eventfd, poll};
use rustix::fs::inotify::{self, CreateFlags, ReadFlags};
use rustix::io::Errno;

use super::{
    ChangeKind, FsChange, Inner, MASK, MAX_RECURSIVE_DIRS, Pending, Service, Subscription, THREADS,
    WatchOptions, file_name, lock,
};

impl Service {
    pub(super) fn start() -> io::Result<Arc<Self>> {
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

    pub(super) fn subscribe(&self, target: PathBuf, options: WatchOptions) -> io::Result<u64> {
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

    pub(super) fn forget(&self, inner: &mut Inner, id: u64) {
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
    pub(super) fn shut_down(&self) {
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
