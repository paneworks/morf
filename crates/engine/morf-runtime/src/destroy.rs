//! Each node's `on_destroyed`, held until the node goes and then queued for
//! a moment handlers can run.

use std::collections::HashMap;

use morf_scene::NodeHandle;

use crate::Handler;

#[derive(Default)]
pub struct DestroyHooks {
    /// Each node's `on_destroyed`, until the node goes.
    hooks: HashMap<NodeHandle, Handler>,
    /// Hooks of nodes already removed, waiting for a moment handlers can run:
    /// removal happens with the state borrowed, often inside a flush.
    pending: Vec<Handler>,
    /// The pending hooks are being run; removals they cause join the queue.
    running: bool,
}

impl DestroyHooks {
    /// Calls `hook` when `node` is removed.
    pub fn set(&mut self, node: NodeHandle, hook: Handler) {
        self.hooks.insert(node, hook);
    }

    /// `node` was removed: its hook, if any, waits to run.
    pub fn removed(&mut self, node: &NodeHandle) {
        if let Some(hook) = self.hooks.remove(node) {
            self.pending.push(hook);
        }
    }

    /// How many hooks wait to run.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Starts running the waiting hooks; false when there are none or they
    /// are running already.
    pub fn begin(&mut self) -> bool {
        if self.running || self.pending.is_empty() {
            return false;
        }
        self.running = true;
        true
    }

    /// The hooks waiting, including those queued while others ran.
    pub fn take_pending(&mut self) -> Vec<Handler> {
        std::mem::take(&mut self.pending)
    }

    /// The waiting hooks have all run.
    pub fn end(&mut self) {
        self.running = false;
    }
}
