//! `ui.watch_transform(a, b, fn)`: pairs of nodes whose relative transform
//! is watched, and who is told when it changes.

use std::collections::{HashMap, HashSet};

use morf_layout::{TransformTracker, TransformWatcher};
use morf_scene::{NodeHandle, Scene};

use crate::Handler;

/// Two nodes whose relative transform is watched.
struct TransformWatch {
    a: NodeHandle,
    b: NodeHandle,
    watcher: TransformWatcher,
    callback: Option<Handler>,
    /// Moves on with every change.
    revision: u64,
    /// A change the callback has not heard yet.
    pending: bool,
}

/// Every watch, by the id its handle carries.
#[derive(Default)]
pub struct TransformWatches {
    watches: HashMap<u64, TransformWatch>,
    next: u64,
}

impl TransformWatches {
    /// How many watches one runtime may hold.
    pub const LIMIT: usize = 1_024;

    /// How many watches there are.
    pub fn len(&self) -> usize {
        self.watches.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.watches.is_empty()
    }

    /// Watches `a` against `b`, within `common_parent` when given; the id.
    pub fn add(
        &mut self,
        a: NodeHandle,
        b: NodeHandle,
        common_parent: Option<NodeHandle>,
        callback: Option<Handler>,
    ) -> u64 {
        let id = self.next;
        self.next = id.wrapping_add(1);
        self.watches.insert(
            id,
            TransformWatch {
                a,
                b,
                watcher: TransformWatcher::new(a, b, common_parent),
                callback,
                revision: 0,
                pending: false,
            },
        );
        id
    }

    /// How many times watch `id` has changed, while it lasts.
    pub fn revision(&self, id: u64) -> Option<u64> {
        self.watches.get(&id).map(|watch| watch.revision)
    }

    /// Drops the watches on removed nodes.
    pub fn forget_nodes(&mut self, removed: &HashSet<NodeHandle>) {
        self.watches
            .retain(|_, watch| !removed.contains(&watch.a) && !removed.contains(&watch.b));
    }

    /// Whether a change waits for a callback to hear it.
    pub fn has_pending_callback(&self) -> bool {
        self.watches
            .values()
            .any(|watch| watch.pending && watch.callback.is_some())
    }

    /// The callbacks owed a change, each with its watch's revision.
    pub fn take_pending(&mut self) -> Vec<(Handler, u64)> {
        let mut owed = Vec::new();
        for watch in self.watches.values_mut() {
            if std::mem::take(&mut watch.pending)
                && let Some(callback) = &watch.callback
            {
                owed.push((callback.clone(), watch.revision));
            }
        }
        owed
    }

    /// Observes every watch against a frame. Returns whether any changed,
    /// and what could not be observed.
    pub fn observe(&mut self, scene: &Scene, tracker: &TransformTracker) -> (bool, Vec<String>) {
        let mut changed = false;
        let mut errors = Vec::new();
        for watch in self.watches.values_mut() {
            match watch.watcher.observe(scene, tracker) {
                Ok(true) => {
                    watch.revision = watch.revision.wrapping_add(1);
                    watch.pending = true;
                    changed = true;
                }
                Ok(false) => {}
                Err(error) => errors.push(format!("transform watcher: {error}")),
            }
        }
        (changed, errors)
    }
}
