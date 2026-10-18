//! What the layout calls into or writes beside the scene: each `ui.Layout`
//! container's functions, and the text nodes whose links it places.

use std::collections::{HashMap, HashSet};

use morf_scene::NodeHandle;

use crate::Handler;

/// A `ui.Layout` container's `measure` and `place` functions.
#[derive(Clone)]
pub struct CustomLayoutFns {
    pub measure: Handler,
    pub place: Handler,
}

#[derive(Default)]
pub struct LayoutHooks {
    custom: HashMap<NodeHandle, CustomLayoutFns>,
    /// Text nodes set in runs, whose links the layout places.
    linked_texts: HashSet<NodeHandle>,
}

impl LayoutHooks {
    /// Lays `node` out with `fns`.
    pub fn set_custom(&mut self, node: NodeHandle, fns: CustomLayoutFns) {
        self.custom.insert(node, fns);
    }

    /// The functions laying `node` out, when it is a `ui.Layout`.
    pub fn custom(&self, node: NodeHandle) -> Option<&CustomLayoutFns> {
        self.custom.get(&node)
    }

    /// Marks `node` as text whose links the layout places.
    pub fn link_text(&mut self, node: NodeHandle) {
        self.linked_texts.insert(node);
    }

    /// Stops placing `node`'s links.
    pub fn unlink_text(&mut self, node: NodeHandle) {
        self.linked_texts.remove(&node);
    }

    /// Every text node whose links the layout places.
    pub fn linked_texts(&self) -> Vec<NodeHandle> {
        self.linked_texts.iter().copied().collect()
    }

    /// Forgets a removed node.
    pub fn forget(&mut self, node: &NodeHandle) {
        self.custom.remove(node);
        self.linked_texts.remove(node);
    }
}
