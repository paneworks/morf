//! Where a key goes when the node with focus does not take keys itself.
//!
//! A button that took focus by Tab has no key handler of its own: Return,
//! the keypad's Enter and Space click it, as they click a button anywhere.
//! Any other key goes up the tree to the nearest ancestor that takes keys --
//! a launcher's arrows and Escape still reach the launcher while one of its
//! rows has focus.

use morf_scene::{Element, NodeHandle};

use crate::runtime_helpers::{handles_keys, takes_keys};
use crate::{EventPoint, KeyModifiers, Runtime, UiEvent};

const RETURN: u32 = 0xff0d;
const KP_ENTER: u32 = 0xff8d;
const SPACE: u32 = 0x20;
/// BTN_LEFT, which a click by key stands in for.
const LEFT_BUTTON: u32 = 0x110;

impl Runtime {
    /// The node a key pressed while `node` has focus goes to: itself when it
    /// takes keys, else its nearest ancestor that does.
    pub fn key_route(&self, node: NodeHandle) -> Option<NodeHandle> {
        let state = self.reactive.borrow();
        let mut current = Some(node);
        while let Some(node) = current {
            if takes_keys(&state, node) {
                return Some(node);
            }
            current = state.scene.parent(node).ok().flatten();
        }
        None
    }

    /// Clicks `node` for Return, Enter or Space with nothing else held, when
    /// it is a pointer area with an `on_clicked` and no keys of its own.
    /// Returns whether it did.
    pub fn activate_by_key(
        &mut self,
        node: NodeHandle,
        keysym: u32,
        modifiers: KeyModifiers,
    ) -> bool {
        let clickable = {
            let state = self.reactive.borrow();
            state.scene.element(node).ok() == Some(Element::MouseArea)
                && !handles_keys(&state, node)
                && state.handlers.contains_key(&(node, UiEvent::Clicked))
        };
        let plain = !modifiers.ctrl && !modifiers.alt && !modifiers.logo;
        if !clickable || !plain || !matches!(keysym, RETURN | KP_ENTER | SPACE) {
            return false;
        }
        let point = EventPoint::new((0.0, 0.0), (0.0, 0.0)).with_button(LEFT_BUTTON);
        self.dispatch_pointer(node, UiEvent::Clicked, point, (0.0, 0.0));
        true
    }
}
