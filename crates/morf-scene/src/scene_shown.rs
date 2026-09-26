//! Whether a change to a node can be seen at all.
//!
//! An animation on a node nothing shows -- itself or an ancestor hidden, or
//! at no opacity -- still advances, but it is not motion: the loop does not
//! keep drawing for it. A shell that eased a hidden bar to a position that
//! ticked every second was in motion for as long as music played, and every
//! output repainted on every frame for a picture that never changed; on a
//! laptop's GPU driving three large screens that starved every program on
//! the desk. Such an animation catches up whenever the loop next turns --
//! when its node is shown, that is a change, and a frame.

use std::collections::HashMap;

use crate::types::{NodeHandle, NodeId, Scene};

impl Scene {
    /// Whether a change to `property` of `node` shows. A node's own opacity
    /// or visibility changing shows whenever its ancestors do: that is how
    /// a fade in from nothing starts. `cache` holds what was found per node
    /// for one tick.
    pub(crate) fn change_shows(
        &self,
        node: NodeId,
        property: &str,
        cache: &mut HashMap<NodeId, bool>,
    ) -> bool {
        let start = if property == "opacity" || property == "visible" {
            match self.parent(NodeHandle(node)) {
                Ok(Some(parent)) => parent.0,
                // A root's own fade: it is the surface, and shows.
                _ => return true,
            }
        } else {
            node
        };
        self.shown(start, cache)
    }

    /// Whether `node` and every ancestor are visible with some opacity.
    fn shown(&self, node: NodeId, cache: &mut HashMap<NodeId, bool>) -> bool {
        if let Some(known) = cache.get(&node) {
            return *known;
        }
        let handle = NodeHandle(node);
        let own = self.bool_value(handle, "visible").unwrap_or(true)
            && self.number(handle, "opacity").unwrap_or(1.0) > 0.0;
        let answer = own
            && match self.parent(handle) {
                Ok(Some(parent)) => self.shown(parent.0, cache),
                _ => true,
            };
        cache.insert(node, answer);
        answer
    }
}
