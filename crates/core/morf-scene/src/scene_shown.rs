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

use crate::FastMap;

use crate::types::{NodeHandle, NodeId, Scene, Value};

impl Scene {
    /// Whether a change to `property` of `node` can be seen now: the
    /// node and its ancestors shown (for its own opacity or visibility,
    /// its ancestors).
    pub fn change_is_shown(&self, node: NodeHandle, property: &str) -> bool {
        let mut current = if property == "opacity" || property == "visible" {
            match self.parent(node) {
                Ok(Some(parent)) => parent,
                _ => return true,
            }
        } else {
            node
        };
        // A single property write needs no cache: each ancestor is visited
        // once. The shared cache below is useful only across an entire tick.
        loop {
            let own = self.bool_value(current, "visible").unwrap_or(true)
                && self.number(current, "opacity").unwrap_or(1.0) > 0.0;
            if !own {
                return false;
            }
            match self.parent(current) {
                Ok(Some(parent)) => current = parent,
                _ => return true,
            }
        }
    }

    /// Eases every colour assigned from now on, by a property's own
    /// behaviour if it has one and by `tween` if not, until it is `None`
    /// again. A theme fading to a new scheme sets it for the one pass that
    /// gives every binding its final colour, so the fade runs here, frame by
    /// frame, and not by running every binding again on every frame.
    pub fn set_colour_tween(&mut self, tween: Option<crate::Behavior>) {
        self.colour_tween = tween;
    }

    /// Whether a new colour for `property` of `node` can skip its motion
    /// and land at once: nothing shows the node, so no frame would draw the
    /// fade, and a new scheme recolouring every hidden page would otherwise
    /// keep that many animations to step and check on every frame.
    pub(crate) fn colour_jumps(&self, node: NodeId, property: &str, value: &Value) -> bool {
        matches!(value, Value::Color(_)) && !self.change_is_shown(NodeHandle(node), property)
    }

    /// Whether a change to `property` of `node` shows. A node's own opacity
    /// or visibility changing shows whenever its ancestors do: that is how
    /// a fade in from nothing starts. `cache` holds what was found per node
    /// for one tick.
    pub(crate) fn change_shows(
        &self,
        node: NodeId,
        property: &str,
        cache: &mut FastMap<NodeId, bool>,
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
    fn shown(&self, node: NodeId, cache: &mut FastMap<NodeId, bool>) -> bool {
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
