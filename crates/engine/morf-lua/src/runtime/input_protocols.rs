//! The text protocols between a configuration and the compositor: the
//! virtual keyboard, the input method, and text-input-v3.

use crate::{runtime::host_types::*, vm::execute::*, vm::types::*};

impl Runtime {
    /// Takes pending virtual keyboard protocol requests.
    pub fn take_virtual_keyboard_requests(&mut self) -> Vec<VirtualKeyboardRequest> {
        self.reactive
            .borrow_mut()
            .requests_mut()
            .take_virtual_keyboard_requests()
    }

    /// Takes whether Lua requested the compositor input-method role.
    pub fn take_input_method_enable_request(&mut self) -> bool {
        self.reactive
            .borrow_mut()
            .requests_mut()
            .take_input_method_enable_request()
    }

    /// Takes pending input-method protocol requests.
    pub fn take_input_method_requests(&mut self) -> Vec<InputMethodRequest> {
        self.reactive
            .borrow_mut()
            .requests_mut()
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
            .requests()
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
            .requests_mut()
            .take_text_input_enable_request()
    }

    /// Takes pending text-input-v3 state requests.
    pub fn take_text_input_requests(&mut self) -> Vec<TextInputRequest> {
        self.reactive
            .borrow_mut()
            .requests_mut()
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
        let callbacks = self
            .reactive
            .borrow()
            .requests()
            .text_input_callbacks
            .clone();
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
}
