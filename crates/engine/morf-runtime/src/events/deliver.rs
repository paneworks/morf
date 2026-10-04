//! Telling a node's handler about an event, and a key passed up the tree
//! while each handler declines it.
//!
//! The runtime decides who hears what and with which arguments; the
//! scripting layer behind [`EventHost`] writes the properties a pointer area
//! keeps and runs the handlers.

use morf_scene::{NodeHandle, Scene};
use morf_value::IpcValue;

use super::routing::{bubbles_keys, key_route, pointer_state_change};
use super::{Events, KeyModifiers, UiEvent};
use crate::Handler;
use crate::editing::key_args;

/// What delivering events needs from whoever runs the handlers.
pub trait EventHost {
    /// Reads the scene and the event table together.
    fn with_events<R>(&self, read: impl FnOnce(&Scene, &Events) -> R) -> R;
    /// Writes a pointer area's `hovered` or `pressed` as a binding would see
    /// it; returns whether it took.
    fn assign_pointer_state(&mut self, node: NodeHandle, property: &str, value: bool) -> bool;
    /// Runs the bindings a write outside any handler made stale.
    fn flush_after_event(&mut self);
    /// Runs an event handler, its return ignored.
    fn run_event_handler(&mut self, handler: &Handler, args: &[IpcValue]) -> Result<(), String>;
    /// Runs a key handler and hands back what it returned.
    fn run_key_handler(
        &mut self,
        handler: &Handler,
        args: &[IpcValue],
    ) -> Result<Vec<IpcValue>, String>;
    /// A key pressed at a node that has the last word on its keys (a text
    /// input, a terminal, a node with no key handler of its own).
    fn press_key(
        &mut self,
        node: NodeHandle,
        keysym: u32,
        text: Option<&str>,
        modifiers: KeyModifiers,
        repeat: bool,
    ) -> bool;
    fn warn(&mut self, message: String);
}

/// Tells `node`'s handler for `event`, called with `args`, after keeping its
/// `hovered` and `pressed` in step. Returns whether anything ran or changed.
pub fn deliver<H: EventHost>(
    host: &mut H,
    node: NodeHandle,
    event: UiEvent,
    args: &[IpcValue],
) -> bool {
    let change = host.with_events(|scene, _| pointer_state_change(scene, node, event));
    let tracked =
        change.is_some_and(|(property, value)| host.assign_pointer_state(node, property, value));
    let Some(handler) = host.with_events(|_, events| events.handler(node, event)) else {
        if tracked {
            host.flush_after_event();
        }
        return tracked;
    };
    if let Err(message) = host.run_event_handler(&handler, args) {
        host.warn(format!("{:?}.{}: {message}", node, event.property()));
    }
    true
}

/// Runs a key press at `node` and, while a handler declines it by returning
/// `false`, at each ancestor that takes keys after it. A text input or a
/// terminal has the last word on its keys. Returns whether anything ran.
pub fn press_key_bubbling<H: EventHost>(
    host: &mut H,
    node: NodeHandle,
    keysym: u32,
    text: Option<&str>,
    modifiers: KeyModifiers,
    repeat: bool,
) -> bool {
    let mut current = Some(node);
    let mut ran = false;
    while let Some(node) = current {
        if !host.with_events(|scene, events| bubbles_keys(scene, events, node)) {
            return ran | host.press_key(node, keysym, text, modifiers, repeat);
        }
        if let Some(handler) =
            host.with_events(|_, events| events.handler(node, UiEvent::KeyPressed))
        {
            ran = true;
            let args = key_args(keysym, text, modifiers, Some(repeat));
            match host.run_key_handler(&handler, &args) {
                Ok(values) if values.first() == Some(&IpcValue::Boolean(false)) => {}
                Ok(_) => return true,
                Err(message) => {
                    host.warn(format!("{node:?}.on_key_pressed: {message}"));
                    return true;
                }
            }
        }
        current = host.with_events(|scene, events| {
            let parent = scene.parent(node).ok().flatten();
            parent.and_then(|parent| key_route(scene, events, parent))
        });
    }
    ran
}
