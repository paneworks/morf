use std::cell::{Ref, RefMut};

use morf_scene::{NodeHandle, Scene};

use crate::{
    events::*, reactive_bindings::*, reactive_execute::*, scene_bindings::*, surface_types::*,
    types::*,
};

impl Runtime {
    /// Borrows the scene produced by executed configuration code.
    pub fn scene(&self) -> Ref<'_, Scene> {
        Ref::map(self.reactive.borrow(), |state| &state.scene)
    }

    /// Mutably borrows the scene for frame-pipeline structural operations.
    pub fn scene_mut(&mut self) -> RefMut<'_, Scene> {
        let mut state = self.reactive.borrow_mut();
        // Native writes must invalidate the same service definitions as Lua
        // writes (for example, a Timer started through the Scene API).
        state.revisions.scene_revision = state.revisions.scene_revision.wrapping_add(1);
        RefMut::map(state, |state| &mut state.scene)
    }

    /// Drains parent transitions queued by Lua handlers.
    pub fn take_parent_transitions(&mut self) -> Vec<ParentTransitionRequest> {
        self.reactive.borrow_mut().windows.take_parent_transitions()
    }

    /// Returns the number of Lua effect evaluations performed by this runtime.
    pub fn effect_runs(&self) -> u64 {
        self.reactive.borrow().effect_runs
    }

    /// Runs one bounded Lua UI handler and retains failures as runtime logs.
    pub fn dispatch_ui_event(&mut self, node: NodeHandle, event: UiEvent) -> bool {
        self.dispatch_ui_event_with_args(node, event, &[])
    }

    /// Returns compositor idle thresholds requested by Lua callbacks, each
    /// with whether it should ignore idle inhibitors.
    pub fn idle_timeouts(&self) -> Vec<(u32, bool)> {
        self.reactive.borrow().requests.idle_timeouts()
    }

    /// The thresholds, when they changed since this was last asked: a
    /// subscription made or cancelled after loading, which the compositor
    /// has to hear about now rather than at the next reload.
    pub fn take_idle_timeouts_change(&mut self) -> Option<Vec<(u32, bool)>> {
        self.reactive
            .borrow_mut()
            .requests
            .take_idle_timeouts_change()
    }

    /// Dispatches one compositor idle state change to registered Lua callbacks.
    pub fn dispatch_idle(&mut self, timeout_ms: u32, input_only: bool, idle: bool) -> bool {
        let callbacks = self
            .reactive
            .borrow()
            .requests
            .idle_subscribers(timeout_ms, input_only);
        for callback in &callbacks {
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_handler_args(ctx, callback, &[IpcValue::Boolean(idle)], limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("idle callback: {message}"));
            }
        }
        !callbacks.is_empty()
    }

    /// Takes pending compositor output power requests.
    /// Takes the `morf.gamma` requests made since the last call, in order.
    pub fn take_gamma_requests(&mut self) -> Vec<crate::GammaRequest> {
        self.reactive.borrow_mut().requests.take_gamma_requests()
    }

    pub fn take_output_power_requests(&mut self) -> Vec<bool> {
        self.reactive
            .borrow_mut()
            .requests
            .take_output_power_requests()
    }

    /// Takes a pending change to whether the session is being held awake.
    pub fn take_idle_inhibit_change(&mut self) -> Option<bool> {
        self.reactive
            .borrow_mut()
            .requests
            .take_idle_inhibit_change()
    }

    /// Takes a pending change to whether the shell wants the compositor's
    /// shortcuts held off it.
    pub fn take_shortcuts_inhibit_change(&mut self) -> Option<bool> {
        self.reactive
            .borrow_mut()
            .requests
            .take_shortcuts_inhibit_change()
    }

    /// Whether the shell currently asks for the compositor's shortcuts to be
    /// held off it (what `morf.shortcuts.inhibit` last said).
    pub fn shortcuts_inhibited(&self) -> bool {
        self.reactive.borrow().requests.shortcuts_inhibited
    }

    /// Delivers the compositor's answer to that request.
    pub fn dispatch_shortcuts_inhibited(&mut self, active: bool) -> bool {
        let callbacks = self.reactive.borrow().requests.shortcuts_callbacks.clone();
        for callback in &callbacks {
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_handler_args(ctx, callback, &[IpcValue::Boolean(active)], limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("shortcuts callback: {message}"));
            }
        }
        !callbacks.is_empty()
    }

    /// Dispatches a compositor clipboard selection to registered Lua callbacks.
    pub fn dispatch_clipboard(&mut self, text: Option<String>) -> bool {
        // Kept for a text input to paste, whether or not anybody subscribed.
        self.reactive.borrow_mut().requests.clipboard_text = text.clone();
        let callbacks = self.reactive.borrow().requests.clipboard_callbacks.clone();
        let value = text.map_or(IpcValue::Nil, IpcValue::String);
        for callback in &callbacks {
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_handler_args(ctx, callback, std::slice::from_ref(&value), limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("clipboard callback: {message}"));
            }
        }
        !callbacks.is_empty()
    }

    /// Tells the configuration the keyboard came to its surface, or left it.
    pub fn dispatch_keyboard_focus(&mut self, active: bool) -> bool {
        let callbacks = self
            .reactive
            .borrow()
            .requests
            .keyboard_focus_callbacks
            .clone();
        let value = IpcValue::Boolean(active);
        for callback in &callbacks {
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_handler_args(ctx, callback, std::slice::from_ref(&value), limits)
            }) {
                self.reactive.borrow_mut().log(
                    LogLevel::Warn,
                    format!("keyboard focus callback: {message}"),
                );
            }
        }
        !callbacks.is_empty()
    }

    /// Tells the configuration the backdrop was clicked: somewhere else.
    pub fn dispatch_backdrop_click(&mut self) -> bool {
        let callbacks = self.reactive.borrow().requests.backdrop_callbacks.clone();
        for callback in &callbacks {
            if let Err(message) =
                self.run_handler(|ctx, limits| execute_handler_args(ctx, callback, &[], limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("backdrop callback: {message}"));
            }
        }
        !callbacks.is_empty()
    }

    /// Takes pending output-capture requests.
    pub fn take_screencopy_requests(&mut self) -> Vec<ScreencopyRequest> {
        self.reactive
            .borrow_mut()
            .requests
            .take_screencopy_requests()
    }

    /// Takes the name a capture asked to be published under, if it chose one.
    pub fn take_screencopy_name(&mut self, request_id: u64) -> Option<String> {
        self.reactive
            .borrow_mut()
            .requests
            .take_screencopy_name(request_id)
    }

    /// Takes the published captures a configuration has released.
    ///
    /// Each is a source string as `frame.source` gave it, or the bare name.
    pub fn take_screencopy_releases(&mut self) -> Vec<String> {
        self.reactive
            .borrow_mut()
            .requests
            .take_screencopy_releases()
    }

    /// Dispatches one output capture to its requesting Lua callback.
    pub fn dispatch_screencopy(
        &mut self,
        request_id: u64,
        result: Result<Screencopy, String>,
    ) -> bool {
        let result = match self.save_screencopy(request_id, result) {
            Ok(handled) => return handled,
            Err(result) => result,
        };
        let Some(callback) = self
            .reactive
            .borrow_mut()
            .requests
            .screencopy_callbacks
            .remove(&request_id)
        else {
            return false;
        };
        if let Err(message) = self
            .run_handler(|ctx, limits| execute_screencopy_handler(ctx, &callback, result, limits))
        {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("screencopy callback: {message}"));
        }
        true
    }

    /// Takes pending virtual keyboard protocol requests.
    pub fn take_virtual_keyboard_requests(&mut self) -> Vec<VirtualKeyboardRequest> {
        self.reactive
            .borrow_mut()
            .requests
            .take_virtual_keyboard_requests()
    }

    /// Takes whether Lua requested the compositor input-method role.
    pub fn take_input_method_enable_request(&mut self) -> bool {
        self.reactive
            .borrow_mut()
            .requests
            .take_input_method_enable_request()
    }

    /// Takes pending input-method protocol requests.
    pub fn take_input_method_requests(&mut self) -> Vec<InputMethodRequest> {
        self.reactive
            .borrow_mut()
            .requests
            .take_input_method_requests()
    }

    /// Dispatches an atomically committed input-method context to Lua.
    pub fn dispatch_input_method(
        &mut self,
        active: bool,
        surrounding_text: Option<String>,
        cursor: u32,
        anchor: u32,
        serial: u32,
    ) -> bool {
        let callbacks = self
            .reactive
            .borrow()
            .requests
            .input_method_callbacks
            .clone();
        let args = [
            IpcValue::Boolean(active),
            surrounding_text.map_or(IpcValue::Nil, IpcValue::String),
            IpcValue::Integer(i64::from(cursor)),
            IpcValue::Integer(i64::from(anchor)),
            IpcValue::Integer(i64::from(serial)),
        ];
        for callback in &callbacks {
            if let Err(message) =
                self.run_handler(|ctx, limits| execute_handler_args(ctx, callback, &args, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("input method callback: {message}"));
            }
        }
        !callbacks.is_empty()
    }

    /// Takes whether Lua requested text-input-v3 creation.
    pub fn take_text_input_enable_request(&mut self) -> bool {
        self.reactive
            .borrow_mut()
            .requests
            .take_text_input_enable_request()
    }

    /// Takes pending text-input-v3 state requests.
    pub fn take_text_input_requests(&mut self) -> Vec<TextInputRequest> {
        self.reactive
            .borrow_mut()
            .requests
            .take_text_input_requests()
    }

    /// Dispatches one atomically committed text-input edit batch to Lua.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_text_input(
        &mut self,
        focused: bool,
        preedit: Option<String>,
        preedit_begin: i32,
        preedit_end: i32,
        commit: Option<String>,
        delete_before: u32,
        delete_after: u32,
        serial: u32,
    ) -> bool {
        // What the input method committed goes into the focused text input
        // before the configuration's own subscribers hear of it.
        let edited = (commit.is_some() || delete_before > 0 || delete_after > 0)
            && self.commit_text_input(commit.as_deref(), delete_before, delete_after);
        let callbacks = self.reactive.borrow().requests.text_input_callbacks.clone();
        let args = [
            IpcValue::Boolean(focused),
            preedit.map_or(IpcValue::Nil, IpcValue::String),
            IpcValue::Integer(i64::from(preedit_begin)),
            IpcValue::Integer(i64::from(preedit_end)),
            commit.map_or(IpcValue::Nil, IpcValue::String),
            IpcValue::Integer(i64::from(delete_before)),
            IpcValue::Integer(i64::from(delete_after)),
            IpcValue::Integer(i64::from(serial)),
        ];
        for callback in &callbacks {
            if let Err(message) =
                self.run_handler(|ctx, limits| execute_handler_args(ctx, callback, &args, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("text input callback: {message}"));
            }
        }
        edited || !callbacks.is_empty()
    }

    /// Returns the first key handler within one scene root.
    pub fn first_key_target_in(&self, root: NodeHandle) -> Option<NodeHandle> {
        let state = self.reactive.borrow();
        routing::first_key_target(&state.scene, &state.events, root)
    }

    /// Every node on a surface that takes keys, in the order they are
    /// offered a key nothing has focus for (one with `focus` set first).
    pub fn key_targets_in_root(&self, root: NodeHandle) -> Vec<NodeHandle> {
        let state = self.reactive.borrow();
        routing::key_offer_order(&state.scene, &state.events, root)
    }

    /// Returns the nearest key-handling ancestor of a hit-tested node.
    pub fn key_target_for_node(&self, node: NodeHandle) -> Option<NodeHandle> {
        let state = self.reactive.borrow();
        routing::key_target_for_node(&state.scene, &state.events, node)
    }

    /// Returns whether a node belongs to the subtree rooted at `root`.
    pub fn node_in_subtree(&self, root: NodeHandle, node: NodeHandle) -> bool {
        let state = self.reactive.borrow();
        routing::node_in_subtree(&state.scene, root, node)
    }

    /// Whether anything has read a node's `contains_pointer`: when nothing
    /// has, a pointer event has no containment to work out.
    pub fn has_pointer_watchers(&self) -> bool {
        !self.reactive.borrow().events.pointer_watch.is_empty()
    }

    /// Every node something has read `contains_pointer` of: the ones the
    /// host tests against the pointer when it moves.
    pub fn pointer_watchers(&self) -> Vec<NodeHandle> {
        self.reactive.borrow().events.pointer_watch.watched()
    }

    /// The nodes first read since this was last asked, for the host to
    /// answer where the pointer is now rather than at its next motion.
    pub fn take_fresh_pointer_watchers(&mut self) -> Vec<NodeHandle> {
        self.reactive.borrow_mut().events.pointer_watch.take_fresh()
    }

    /// Records whether the pointer is inside each node, as the host worked
    /// it out. A node nothing has read is ignored. The bindings that read
    /// one that changed run at the next flush -- a handler's return, or
    /// [`Runtime::flush_after_event`]. Returns whether any changed.
    pub fn set_contains_pointer(&mut self, answers: &[(NodeHandle, bool)]) -> bool {
        let mut changed = false;
        {
            let mut state = self.reactive.borrow_mut();
            for node in state.events.pointer_watch.answer(answers) {
                changed = true;
                state.flush_pending = true;
                let _ = bump_property_signal(&mut state, node, CONTAINS_POINTER, false);
            }
        }
        changed
    }

    /// Runs the bindings a host-side write made stale, when no handler
    /// will: the flush a handler's return would otherwise have been.
    pub fn flush_after_event(&mut self) {
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
                .log(LogLevel::Warn, format!("after pointer: {message}"));
        }
    }

    /// Whether Tab pressed while `node` has the keyboard moves focus on
    /// (its `tab_navigation`, true unless it said otherwise), rather than
    /// going to the node as a key.
    pub fn tab_navigates(&self, node: NodeHandle) -> bool {
        self.reactive
            .borrow()
            .scene
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
        routing::next_key_target(&state.scene, &state.events, root, current)
    }

    /// Tells `node`'s handler for `event`; see [`morf_runtime::events::deliver`].
    pub(crate) fn dispatch_ui_event_with_args(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        args: &[IpcValue],
    ) -> bool {
        deliver(self, node, event, args)
    }
}
