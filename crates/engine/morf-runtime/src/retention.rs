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

impl Default for Retained {
    fn default() -> Self {
        Self {
            loader_factories: HashMap::new(),
            failed_loaders: HashSet::new(),
            loaded_loaders: HashSet::new(),
            dormant_loaders: HashSet::new(),
            preload_pending: HashMap::new(),
            retention: Retention::default(),
            retain_callbacks: HashMap::new(),
            retained_destroy_queue: HashSet::new(),
        }
    }
}
