//! Where a key goes: which nodes on a surface take keys, how focus moves
//! between them, and where a key goes when the node with focus does not take
//! keys itself.
//!
//! A button that took focus by Tab has no key handler of its own: Return,
//! the keypad's Enter and Space click it, as they click a button anywhere.
//! Any other key goes up the tree to the nearest ancestor that takes keys --
//! a launcher's arrows and Escape still reach the launcher while one of its
//! rows has focus. A key handler that returns `false` passes the key on
//! the same way: to the next ancestor that takes keys.

use morf_scene::NodeHandle;

use crate::runtime::events::{press_key_bubbling, routing};
use crate::{EventPoint, KeyModifiers, Runtime, UiEvent};

/// BTN_LEFT, which a click by key stands in for.
const LEFT_BUTTON: u32 = 0x110;

impl Runtime {
    /// The node a key pressed while `node` has focus goes to: itself when it
    /// takes keys, else its nearest ancestor that does.
    pub fn key_route(&self, node: NodeHandle) -> Option<NodeHandle> {
        self.reactive.borrow().key_route(node)
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
        let activates = {
            let state = self.reactive.borrow();
            routing::activates_by_key(state.scene(), state.events(), node, keysym, modifiers)
        };
        if !activates {
            return false;
        }
        let point = EventPoint::new((0.0, 0.0), (0.0, 0.0)).with_button(LEFT_BUTTON);
        self.dispatch_pointer(node, UiEvent::Clicked, point, (0.0, 0.0));
        true
    }

    /// Runs a key press at `node` and, while a handler declines it by
    /// returning `false`, at each ancestor that takes keys after it. A text
    /// input or a terminal has the last word on its keys. Returns whether
    /// anything ran.
    pub fn dispatch_key_press_bubbling(
        &mut self,
        node: NodeHandle,
        keysym: u32,
        text: Option<&str>,
        modifiers: KeyModifiers,
        repeat: bool,
    ) -> bool {
        press_key_bubbling(self, node, keysym, text, modifiers, repeat)
    }

    /// Returns the first key handler within one scene root.
    pub fn first_key_target_in(&self, root: NodeHandle) -> Option<NodeHandle> {
        let state = self.reactive.borrow();
        routing::first_key_target(state.scene(), state.events(), root)
    }

    /// Every node on a surface that takes keys, in the order they are
    /// offered a key nothing has focus for (one with `focus` set first).
    pub fn key_targets_in_root(&self, root: NodeHandle) -> Vec<NodeHandle> {
        let state = self.reactive.borrow();
        routing::key_offer_order(state.scene(), state.events(), root)
    }

    /// Returns the nearest key-handling ancestor of a hit-tested node.
    pub fn key_target_for_node(&self, node: NodeHandle) -> Option<NodeHandle> {
        let state = self.reactive.borrow();
        routing::key_target_for_node(state.scene(), state.events(), node)
    }

    /// Returns whether a node belongs to the subtree rooted at `root`.
    pub fn node_in_subtree(&self, root: NodeHandle, node: NodeHandle) -> bool {
        let state = self.reactive.borrow();
        routing::node_in_subtree(state.scene(), root, node)
    }

    /// Whether Tab pressed while `node` has the keyboard moves focus on
    /// (its `tab_navigation`, true unless it said otherwise), rather than
    /// going to the node as a key.
    pub fn tab_navigates(&self, node: NodeHandle) -> bool {
        self.reactive
            .borrow()
            .scene()
            .bool_value(node, "tab_navigation")
            .unwrap_or(true)
    }

    /// Advances keyboard focus within one scene root.
    pub fn next_key_target_in(
        &self,
        root: NodeHandle,
        current: Option<NodeHandle>,
    ) -> Option<NodeHandle> {
        let state = self.reactive.borrow();
        routing::next_key_target(state.scene(), state.events(), root, current)
    }
}
