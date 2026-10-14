//! What a configuration asks of the compositor -- idle thresholds, gamma,
//! output power, inhibitors, captures -- and the answers it is told.

use crate::{runtime::host_types::*, vm::execute::*, vm::types::*};

impl Runtime {
    /// Returns compositor idle thresholds requested by Lua callbacks, each
    /// with whether it should ignore idle inhibitors.
    pub fn idle_timeouts(&self) -> Vec<(u32, bool)> {
        self.reactive.borrow().requests().idle_timeouts()
    }

    /// The thresholds, when they changed since this was last asked: a
    /// subscription made or cancelled after loading, which the compositor
    /// has to hear about now rather than at the next reload.
    pub fn take_idle_timeouts_change(&mut self) -> Option<Vec<(u32, bool)>> {
        self.reactive
            .borrow_mut()
            .requests_mut()
            .take_idle_timeouts_change()
    }

    /// Dispatches one compositor idle state change to registered Lua callbacks.
    pub fn dispatch_idle(&mut self, timeout_ms: u32, input_only: bool, idle: bool) -> bool {
        let callbacks = self
            .reactive
            .borrow()
            .requests()
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
        self.reactive
            .borrow_mut()
            .requests_mut()
            .take_gamma_requests()
    }

    pub fn take_output_power_requests(&mut self) -> Vec<bool> {
        self.reactive
            .borrow_mut()
            .requests_mut()
            .take_output_power_requests()
    }

    /// Takes a pending change to whether the session is being held awake.
    pub fn take_idle_inhibit_change(&mut self) -> Option<bool> {
        self.reactive
            .borrow_mut()
            .requests_mut()
            .take_idle_inhibit_change()
    }

    /// Takes a pending change to whether the shell wants the compositor's
    /// shortcuts held off it.
    pub fn take_shortcuts_inhibit_change(&mut self) -> Option<bool> {
        self.reactive
            .borrow_mut()
            .requests_mut()
            .take_shortcuts_inhibit_change()
    }

    /// Whether the shell currently asks for the compositor's shortcuts to be
    /// held off it (what `morf.shortcuts.inhibit` last said).
    pub fn shortcuts_inhibited(&self) -> bool {
        self.reactive.borrow().requests().shortcuts_inhibited
    }

    /// Delivers the compositor's answer to that request.
    pub fn dispatch_shortcuts_inhibited(&mut self, active: bool) -> bool {
        let callbacks = self
            .reactive
            .borrow()
            .requests()
            .shortcuts_callbacks
            .clone();
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

    /// Tells the configuration the keyboard came to its surface, or left it.
    pub fn dispatch_keyboard_focus(&mut self, active: bool) -> bool {
        let callbacks = self
            .reactive
            .borrow()
            .requests()
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
        let callbacks = self.reactive.borrow().requests().backdrop_callbacks.clone();
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
            .requests_mut()
            .take_screencopy_requests()
    }

    /// Takes the name a capture asked to be published under, if it chose one.
    pub fn take_screencopy_name(&mut self, request_id: u64) -> Option<String> {
        self.reactive
            .borrow_mut()
            .requests_mut()
            .take_screencopy_name(request_id)
    }

    /// Takes the published captures a configuration has released.
    ///
    /// Each is a source string as `frame.source` gave it, or the bare name.
    pub fn take_screencopy_releases(&mut self) -> Vec<String> {
        self.reactive
            .borrow_mut()
            .requests_mut()
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
            .requests_mut()
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
}
