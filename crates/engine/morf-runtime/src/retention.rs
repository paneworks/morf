//! Nodes that outlive their place in the tree: loaders' items, built
//! ahead or kept after, and nodes retained past their parent, with what each
//! is told as it goes.

use std::collections::{HashMap, HashSet};

use morf_scene::NodeHandle;
use morf_scene::retain::Retention;

use crate::Handler;

/// What a retained node is told: when it is dropped, and just before it
/// is destroyed.
#[derive(Clone, Default)]
pub struct RetainCallbacks {
    pub dropped: Option<Handler>,
    pub about_to_destroy: Option<Handler>,
}

/// Loaders' items and retained nodes, and the handlers told as they go.
#[derive(Default)]
pub struct Retained {
    /// Each loader's source: what builds its item.
    pub loader_factories: HashMap<NodeHandle, Handler>,
    /// Loaders whose source raised, left alone until they are deactivated.
    pub failed_loaders: HashSet<NodeHandle>,
    /// Loaders whose item is built.
    pub loaded_loaders: HashSet<NodeHandle>,
    /// Loaders holding an item that is built but not shown: preloaded ahead
    /// of being asked for, or kept after being let go.
    pub dormant_loaders: HashSet<NodeHandle>,
    /// Preloading loaders with nothing built yet, and since when.
    pub preload_pending: HashMap<NodeHandle, std::time::Instant>,
    /// Nodes kept alive past their parent, and what holds each.
    pub retention: Retention<NodeHandle>,
    /// What each retained node is told as it is let go.
    pub retain_callbacks: HashMap<NodeHandle, RetainCallbacks>,
    /// Retained nodes to destroy once Lua can run.
    pub retained_destroy_queue: HashSet<NodeHandle>,
}

impl Retained {
    /// Forgets a removed node: its retention, its callbacks and any loader
    /// bookkeeping it had.
    pub fn forget(&mut self, node: &NodeHandle) {
        self.retention.unregister(*node);
        self.retain_callbacks.remove(node);
        self.loader_factories.remove(node);
        self.failed_loaders.remove(node);
        self.loaded_loaders.remove(node);
        self.dormant_loaders.remove(node);
        self.preload_pending.remove(node);
    }

    /// Starts dropping a retainable: who to tell.
    pub fn begin_drop(&mut self, node: NodeHandle) -> Option<Handler> {
        let _ = self.retention.begin_drop(node);
        self.retain_callbacks
            .get(&node)
            .and_then(|callbacks| callbacks.dropped.clone())
    }

    /// Whether nothing holds a node any more (or it was never held).
    pub fn should_destroy(&self, node: NodeHandle) -> bool {
        self.retention.should_destroy(node).unwrap_or(true)
    }

    /// Who to tell just before a retained node is destroyed.
    pub fn about_to_destroy(&self, node: NodeHandle) -> Option<Handler> {
        self.retain_callbacks
            .get(&node)
            .and_then(|callbacks| callbacks.about_to_destroy.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use morf_scene::{Element, Scene};

    #[test]
    fn a_retainable_is_held_until_dropped_and_forgotten_with_its_loader() {
        let mut scene = Scene::default();
        let node = scene.create(Element::Item);
        let mut retained = Retained::default();
        assert!(retained.should_destroy(node), "never held");
        retained.retention.register(node);
        let _ = retained.retention.lock(node);
        assert!(retained.begin_drop(node).is_none(), "no callback given");
        assert!(retained.about_to_destroy(node).is_none());
        assert!(!retained.should_destroy(node), "still locked");
        let _ = retained.retention.unlock(node);
        assert!(retained.should_destroy(node));
        retained.loaded_loaders.insert(node);
        retained.forget(&node);
        assert!(retained.loaded_loaders.is_empty());
        assert!(retained.retention.state(node).is_none());
    }
}
