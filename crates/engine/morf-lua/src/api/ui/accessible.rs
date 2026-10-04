//! What a screen reader may do to a node, carried out as the node's own
//! behaviour.
//!
//! A node's `on_accessible_action(action, value)` answers first -- a kit
//! slider sets its value, a field its text -- and returning anything but
//! `false` ends it. Otherwise the action is the key a keyboard user would
//! press: `"click"` Space (or the click a pointer area's Return makes),
//! `"increment"` and `"decrement"` Up and Down, `"expand"` and `"collapse"`
//! Right and Left; `"focus"` gives it focus. The platform side (AT-SPI,
//! through AccessKit) lives with the surfaces; this knows none of it.

use morf_scene::NodeHandle;

use crate::api_focus::FocusReason;
use crate::reactive_execute::execute_ipc_handler;
use crate::types::LogLevel;
use crate::{IpcValue, KeyModifiers, Runtime, UiEvent};

const SPACE: u32 = 0x20;
const LEFT: u32 = 0xff51;
const UP: u32 = 0xff52;
const RIGHT: u32 = 0xff53;
const DOWN: u32 = 0xff54;

impl Runtime {
    /// The scene's revision: it moves whenever a property is written, so
    /// a backend knows when a tree it built may be stale.
    pub fn scene_revision(&self) -> u64 {
        self.reactive.borrow().scene_revision
    }

    /// Carries out a screen reader's `action` on `node`, under the surface
    /// root `root`. Returns whether anything ran.
    pub fn accessible_action(
        &mut self,
        root: NodeHandle,
        node: NodeHandle,
        action: &str,
        value: Option<IpcValue>,
    ) -> bool {
        if !self.reactive.borrow().scene.contains(node) {
            return false;
        }
        let handler = self
            .reactive
            .borrow()
            .events
            .handler(node, UiEvent::AccessibleAction);
        if let Some(handler) = handler {
            let args = vec![
                IpcValue::from(action),
                value.clone().unwrap_or(IpcValue::Nil),
            ];
            match self.run_handler(|ctx, limits| execute_ipc_handler(ctx, &handler, &args, limits))
            {
                Ok(values) if values.first() == Some(&IpcValue::Boolean(false)) => {}
                Ok(_) => {
                    self.flush_after_event();
                    return true;
                }
                Err(message) => {
                    self.reactive.borrow_mut().log(
                        LogLevel::Warn,
                        format!("{node:?}.on_accessible_action: {message}"),
                    );
                    return true;
                }
            }
        }
        let key = match action {
            "focus" => return self.set_focus(root, Some(node), FocusReason::Keyboard),
            "click" => SPACE,
            "increment" => UP,
            "decrement" => DOWN,
            "expand" => RIGHT,
            "collapse" => LEFT,
            _ => return false,
        };
        // What it does by key it does with focus, as a keyboard user's would.
        self.set_focus(root, Some(node), FocusReason::Keyboard);
        let plain = KeyModifiers::default();
        if self.activate_by_key(node, key, plain) {
            return true;
        }
        let Some(target) = self.key_route(node) else {
            return false;
        };
        let text = (key == SPACE).then_some(" ");
        let ran = self.dispatch_key_press_bubbling(target, key, text, plain, false);
        self.dispatch_key_release(target, key, text, plain);
        ran
    }
}
