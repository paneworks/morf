//! A runtime's watches, each with the callback it owes, drained a batch at
//! a time.
//!
//! The hub does not know what a callback is: it keeps whatever the caller
//! registers (`C`) beside each [`Watch`], and [`WatchHub::collect`] hands
//! back the calls owed — callback, change and the watch's shared status —
//! for the caller to run. At most [`WATCH_BATCH`] per watch per turn, so one
//! busy folder cannot starve the rest; the rest stays queued for the next.
//! A watch is closed through its [`WatchStatus`], and the hub drops it, and
//! the kernel's watch with it, on its next turn.

use std::cell::Cell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;

use crate::{FsChange, Watch};

/// Callbacks one watch may have run per turn of the loop.
pub const WATCH_BATCH: usize = 64;

/// What a handle and its entry share.
#[derive(Debug, Default)]
pub struct WatchStatus {
    closed: Cell<bool>,
}

impl WatchStatus {
    pub fn close(&self) {
        self.closed.set(true);
    }

    pub fn closed(&self) -> bool {
        self.closed.get()
    }
}

/// What a caller keeps for an open watch: its status and its path.
#[derive(Debug)]
pub struct WatchHandle {
    pub status: Rc<WatchStatus>,
    pub path: PathBuf,
}

impl Drop for WatchHandle {
    /// Dropped: nobody can close it any more, so it closes itself.
    fn drop(&mut self) {
        self.status.close();
    }
}

struct Entry<C> {
    watch: Watch,
    callback: C,
    status: Rc<WatchStatus>,
    queue: VecDeque<FsChange>,
}

/// A callback owed.
pub struct WatchCall<C> {
    pub status: Rc<WatchStatus>,
    pub callback: C,
    pub change: FsChange,
}

impl<C> WatchCall<C> {
    /// Whether the call should still run: its watch may have been closed
    /// meanwhile, by an earlier callback in the same turn, say.
    pub fn live(&self) -> bool {
        !self.status.closed()
    }
}

/// The runtime's watches.
pub struct WatchHub<C> {
    entries: Vec<Entry<C>>,
}

impl<C> Default for WatchHub<C> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<C: Clone> WatchHub<C> {
    /// Open watches.
    pub fn len(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| !entry.status.closed())
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Refuses one more watch past `limit` open ones.
    pub fn check_room(&self, limit: usize) -> Result<(), String> {
        if self.len() >= limit {
            return Err(format!(
                "more than {limit} watches open (MORF_LIMITS=watches=N)"
            ));
        }
        Ok(())
    }

    /// Keeps `watch` with the `callback` it owes; the handle closes it.
    pub fn add(&mut self, watch: Watch, callback: C) -> WatchHandle {
        let status = Rc::new(WatchStatus::default());
        let handle = WatchHandle {
            status: Rc::clone(&status),
            path: watch.path().to_path_buf(),
        };
        self.entries.push(Entry {
            watch,
            callback,
            status,
            queue: VecDeque::new(),
        });
        handle
    }

    /// What each watch gathered, as the callbacks it is owed; up to
    /// [`WATCH_BATCH`] per watch. The second value says more is waiting.
    pub fn collect(&mut self) -> (Vec<WatchCall<C>>, bool) {
        let mut calls = Vec::new();
        let mut more = false;
        self.entries.retain_mut(|entry| {
            if entry.status.closed() {
                return false;
            }
            entry.queue.extend(entry.watch.drain());
            for _ in 0..WATCH_BATCH {
                let Some(change) = entry.queue.pop_front() else {
                    break;
                };
                calls.push(WatchCall {
                    status: Rc::clone(&entry.status),
                    callback: entry.callback.clone(),
                    change,
                });
            }
            more |= !entry.queue.is_empty();
            true
        });
        (calls, more)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WatchOptions;

    #[test]
    fn handles_close_their_watches_and_the_hub_lets_go() {
        let dir = std::env::temp_dir().join(format!("morf-watch-hub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut hub = WatchHub::<u32>::default();
        let first = hub.add(Watch::new(&dir, WatchOptions::default()).unwrap(), 1);
        let second = hub.add(Watch::new(&dir, WatchOptions::default()).unwrap(), 2);
        assert_eq!(hub.len(), 2);
        assert!(hub.check_room(2).is_err());
        assert!(hub.check_room(3).is_ok());
        first.status.close();
        assert_eq!(hub.len(), 1);
        drop(second);
        assert!(hub.is_empty());
        let (calls, more) = hub.collect();
        assert!(calls.is_empty() && !more);
        assert!(hub.entries.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
