//! Where a key goes when the node with focus does not take keys itself.
//!
//! A button that took focus by Tab has no key handler of its own: Return,
//! the keypad's Enter and Space click it, as they click a button anywhere.
//! Any other key goes up the tree to the nearest ancestor that takes keys --
//! a launcher's arrows and Escape still reach the launcher while one of its
//! rows has focus. A key handler that returns `false` passes the key on
//! the same way: to the next ancestor that takes keys.

use morf_scene::{Element, NodeHandle};

use crate::reactive_execute::execute_ipc_handler;
use crate::runtime_helpers::{handles_keys, takes_keys};
use crate::types::LogLevel;
use crate::{EventPoint, IpcValue, KeyModifiers, Runtime, UiEvent};

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
        let mut current = Some(node);
        let mut ran = false;
        while let Some(node) = current {
            let plain = {
                let state = self.reactive.borrow();
                handles_keys(&state, node)
                    && !matches!(
                        state.scene.element(node).ok(),
                        Some(Element::TextInput | Element::Terminal)
                    )
            };
            if !plain {
                return ran | self.dispatch_key_press(node, keysym, text, modifiers, repeat);
            }
            let handler = self
                .reactive
                .borrow()
                .handlers
                .get(&(node, UiEvent::KeyPressed))
                .cloned();
            if let Some(handler) = handler {
                ran = true;
                let args =
                    crate::runtime_text_inputs::key_args(keysym, text, modifiers, Some(repeat));
                match self
                    .run_handler(|ctx, limits| execute_ipc_handler(ctx, &handler, &args, limits))
                {
                    Ok(values) if values.first() == Some(&IpcValue::Boolean(false)) => {}
                    Ok(_) => return true,
                    Err(message) => {
                        self.reactive.borrow_mut().log(
                            LogLevel::Warn,
                            format!("{node:?}.on_key_pressed: {message}"),
                        );
                        return true;
                    }
                }
            }
            let parent = self.reactive.borrow().scene.parent(node).ok().flatten();
            current = parent.and_then(|parent| self.key_route(parent));
        }
        ran
    }
}
