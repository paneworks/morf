//! The runtime's side of text inputs: keys and frames in, callbacks out.

use std::time::Instant;

use morf_layout::{InputShape, Layout, TextMeasurer};
use morf_scene::{Element, NodeHandle};
use morf_text::TextSystem;

use crate::text_inputs::{self, KeyModifiers, KeyOutcome};
use crate::{events::*, reactive_bindings::*, runtime_helpers::*, surface_types::*, types::*};

/// How many rounds of callbacks one event may set off before the rest are
/// dropped: a handler that edits its own field from `on_text_changed` would
/// otherwise go round for ever.
const CALLBACK_ROUNDS: usize = 16;

impl Runtime {
    /// Whether a node is a text input.
    pub fn is_text_input(&self, node: NodeHandle) -> bool {
        self.reactive.borrow().scene.element(node).ok() == Some(Element::TextInput)
    }

    /// The text input that has the keyboard within one scene root, if any.
    pub fn focused_text_input_in(&mut self, root: NodeHandle) -> Option<NodeHandle> {
        self.settle_text_inputs();
        let state = self.reactive.borrow();
        state.focused_input.filter(|node| {
            scene_node_in_subtree(&state.scene, root, *node)
                && state.scene.bool_value(*node, "enabled").unwrap_or(false)
                && state.scene.bool_value(*node, "visible").unwrap_or(false)
        })
    }

    /// Tells the runtime which node the keyboard went to by a click or a Tab.
    ///
    /// A text input takes it; anything else that handles keys takes it away
    /// from whichever text input had it. Returns whether that changed
    /// anything worth a frame.
    pub fn set_key_focus(&mut self, node: Option<NodeHandle>) -> bool {
        let changed = {
            let mut state = self.reactive.borrow_mut();
            let before = state.focused_input;
            match node {
                Some(node) if state.scene.element(node).ok() == Some(Element::TextInput) => {
                    text_inputs::set_focus(&mut state, node, true);
                }
                _ => {
                    if let Some(focused) = state.focused_input {
                        text_inputs::set_focus(&mut state, focused, false);
                    }
                }
            }
            state.focused_input != before
        };
        self.finish_text_input_work();
        changed
    }

    /// Runs one key press: into the focused text input when `node` is one,
    /// and to the node's `on_key_pressed` otherwise — or when the text input
    /// had no use for it.
    ///
    /// A handler is called as `(keysym, text, modifiers, repeat)`, the third
    /// a string such as `"ctrl+shift"`, empty when nothing is held, and the
    /// last false here; see [`Runtime::dispatch_key_press`] for repeats.
    pub fn dispatch_key(
        &mut self,
        node: NodeHandle,
        keysym: u32,
        text: Option<&str>,
        modifiers: KeyModifiers,
    ) -> bool {
        self.dispatch_key_press(node, keysym, text, modifiers, false)
    }

    /// Runs one key press, saying whether it is the keyboard's own repeat of
    /// a held key rather than a fresh press.
    ///
    /// A text input takes a repeat as it takes a press — a held Backspace
    /// keeps deleting — while a handler can tell them apart by its fourth
    /// argument: a game moving on held keys ignores repeats and tracks the
    /// press and its release instead.
    pub fn dispatch_key_press(
        &mut self,
        node: NodeHandle,
        keysym: u32,
        text: Option<&str>,
        modifiers: KeyModifiers,
        repeat: bool,
    ) -> bool {
        let outcome = if self.is_text_input(node) {
            let outcome = {
                let mut state = self.reactive.borrow_mut();
                text_inputs::reconcile_focus(&mut state);
                text_inputs::key(&mut state, node, keysym, text, modifiers)
            };
            self.finish_text_input_work();
            outcome
        } else {
            KeyOutcome::Ignored
        };
        if outcome == KeyOutcome::Handled {
            return true;
        }
        self.dispatch_ui_event_with_args(
            node,
            UiEvent::KeyPressed,
            &key_args(keysym, text, modifiers, Some(repeat)),
        )
    }

    /// Runs one key release to the node's `on_key_released`, called as
    /// `(keysym, text, modifiers)`.
    ///
    /// Text inputs have no use for releases, so one goes to the handler
    /// whatever the node is.
    pub fn dispatch_key_release(
        &mut self,
        node: NodeHandle,
        keysym: u32,
        text: Option<&str>,
        modifiers: KeyModifiers,
    ) -> bool {
        self.dispatch_ui_event_with_args(
            node,
            UiEvent::KeyReleased,
            &key_args(keysym, text, modifiers, None),
        )
    }

    /// Lays each text input's shaped text against its box: keeps its caret in
    /// view, publishes its content size, and keeps the caret stops the next
    /// click and the next arrow key are answered from.
    ///
    /// Called once a frame, after layout and before paint, with the text
    /// system that frame is painted with — so the caret it scrolls to is
    /// where the glyphs will be drawn. Returns whether it changed anything.
    pub fn sync_text_inputs(&mut self, layout: &Layout, text: &mut TextSystem) -> bool {
        let revision = self.reactive.borrow().scene_revision;
        {
            let mut state = self.reactive.borrow_mut();
            text_inputs::reconcile_focus(&mut state);
            for node in text_inputs::tracked(&state) {
                let Some(geometry) = layout.geometry(node) else {
                    continue;
                };
                let Ok(shape) = InputShape::read(&state.scene, node, Some(geometry.width)) else {
                    continue;
                };
                // The same string at the same width paint will shape, so this
                // is the buffer paint draws, not a second one.
                let measured = text.measure(
                    node,
                    &shape.display.text,
                    &shape.family,
                    shape.size,
                    shape.options.clone(),
                );
                let map = text.caret_map(node).unwrap_or_default();
                text_inputs::observe_shaped(
                    &mut state,
                    node,
                    geometry,
                    &shape.display.text,
                    map,
                    measured.height,
                );
            }
            text_inputs::blink(&mut state, Instant::now());
        }
        self.finish_text_input_work();
        self.reactive.borrow().scene_revision != revision
    }

    /// Moves the focused caret's blink on; true when it changed.
    pub(crate) fn blink_text_inputs(&mut self) -> bool {
        let blinked = {
            let mut state = self.reactive.borrow_mut();
            text_inputs::reconcile_focus(&mut state);
            text_inputs::blink(&mut state, Instant::now())
        };
        self.finish_text_input_work();
        blinked
    }

    /// Picks up `focus` writes the configuration made, and runs what they owe.
    fn settle_text_inputs(&mut self) {
        text_inputs::reconcile_focus(&mut self.reactive.borrow_mut());
        self.finish_text_input_work();
    }

    /// Applies an input method's commit to the focused text input.
    pub(crate) fn commit_text_input(
        &mut self,
        commit: Option<&str>,
        before: u32,
        after: u32,
    ) -> bool {
        let edited = {
            let mut state = self.reactive.borrow_mut();
            text_inputs::reconcile_focus(&mut state);
            text_inputs::input_method_commit(&mut state, commit, before, after)
        };
        self.finish_text_input_work();
        edited
    }

    /// Flushes what editing wrote, then runs the callbacks it owes.
    pub(crate) fn finish_text_input_work(&mut self) {
        let flush = {
            let mut state = self.reactive.borrow_mut();
            state.handler_depth == 0 && std::mem::take(&mut state.flush_pending)
        };
        if flush
            && let Err(message) = self
                .lua
                .enter(|ctx| flush_reactive(&self.reactive, ctx, self.limits))
        {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("text input: {message}"));
        }
        self.drain_input_events();
    }

    /// Runs the callbacks text inputs have queued, in the order they were
    /// owed. A callback that edits a field queues more; those run too, up to
    /// a bound.
    pub(crate) fn drain_input_events(&mut self) {
        {
            let mut state = self.reactive.borrow_mut();
            if state.draining_input_events
                || state.handler_depth > 0
                || state.input_events.is_empty()
            {
                return;
            }
            state.draining_input_events = true;
        }
        for _ in 0..CALLBACK_ROUNDS {
            let events = std::mem::take(&mut self.reactive.borrow_mut().input_events);
            if events.is_empty() {
                break;
            }
            for (node, event, args) in events {
                self.dispatch_ui_event_with_args(node, event, &args);
            }
            text_inputs::reconcile_focus(&mut self.reactive.borrow_mut());
        }
        let mut state = self.reactive.borrow_mut();
        state.draining_input_events = false;
        if !state.input_events.is_empty() {
            state.input_events.clear();
            state.log(
                LogLevel::Warn,
                "text input callbacks kept editing their own fields; the rest were dropped",
            );
        }
    }
}

/// The arguments a key handler is called with; `repeat` only for presses.
fn key_args(
    keysym: u32,
    text: Option<&str>,
    modifiers: KeyModifiers,
    repeat: Option<bool>,
) -> Vec<IpcValue> {
    let mut args = vec![
        IpcValue::Integer(i64::from(keysym)),
        text.map_or(IpcValue::Nil, |value| IpcValue::String(value.to_owned())),
        IpcValue::String(modifiers.name()),
    ];
    if let Some(repeat) = repeat {
        args.push(IpcValue::Boolean(repeat));
    }
    args
}
