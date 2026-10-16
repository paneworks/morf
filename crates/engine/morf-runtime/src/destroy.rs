//! Each node's `on_destroyed`, and the hooks of what a node owns, held until
//! the node goes and then queued for a moment handlers can run.

use std::collections::HashMap;

use morf_scene::NodeHandle;

use crate::Handler;

#[derive(Default)]
pub struct DestroyHooks {
    /// Each node's `on_destroyed`, until the node goes.
    hooks: HashMap<NodeHandle, Handler>,
    /// Teardowns of what a node owns (a bound service), beside its
    /// `on_destroyed`, each by the id its owner can let go of it by.
    owned: HashMap<NodeHandle, Vec<(u64, Handler)>>,
    next_owned: u64,
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

    /// Also calls `hook` when `node` is removed, after its `on_destroyed`.
    /// Returns the id [`Self::disown`] takes.
    pub fn own(&mut self, node: NodeHandle, hook: Handler) -> u64 {
        self.next_owned += 1;
        self.owned
            .entry(node)
            .or_default()
            .push((self.next_owned, hook));
        self.next_owned
    }

    /// Forgets owned hook `id` before its node goes. False if it was gone.
    pub fn disown(&mut self, id: u64) -> bool {
        let Some((node, index)) = self.owned.iter().find_map(|(node, hooks)| {
            hooks
                .iter()
                .position(|(owned, _)| *owned == id)
                .map(|index| (*node, index))
        }) else {
            return false;
        };
        let hooks = self.owned.get_mut(&node).expect("found above");
        hooks.remove(index);
        if hooks.is_empty() {
            self.owned.remove(&node);
        }
        true
    }

    /// How many owned hooks wait for their nodes.
    pub fn owned_count(&self) -> usize {
        self.owned.values().map(Vec::len).sum()
    }

    /// `node` was removed: its hooks, if any, wait to run.
    pub fn removed(&mut self, node: &NodeHandle) {
        if let Some(hook) = self.hooks.remove(node) {
            self.pending.push(hook);
        }
        if let Some(owned) = self.owned.remove(node) {
            self.pending.extend(owned.into_iter().map(|(_, hook)| hook));
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
