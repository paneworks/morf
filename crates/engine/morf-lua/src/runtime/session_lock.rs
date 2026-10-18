//! Where the session lock stands, as the compositor tells it.
//!
//! A lock client asks for the lock and the compositor answers later: it
//! confirms once every output shows a locked frame, or refuses, or ends the
//! lock of its own accord. The configuration hears each of these through
//! `morf.session_lock` (a signal), `morf.session_lock_state()`, and the
//! `morf.on_session_lock_state` / `morf.on_session_locked` callbacks.

use crate::{api::ui::bindings::*, runtime::state::*, vm::execute::*, vm::types::*};

pub use morf_runtime::session::SessionLockState;

/// The state as last recorded.
pub(crate) fn current(state: &ReactiveState) -> SessionLockState {
    state.session_lock_state()
}

impl Runtime {
    /// Where this process's session lock stands.
    pub fn session_lock_state(&self) -> SessionLockState {
        current(&self.reactive.borrow())
    }

    /// Records what the compositor said about the lock and tells the
    /// configuration, once per change. Returns whether anything changed.
    pub fn set_session_lock_state(&mut self, next: SessionLockState) -> bool {
        let value = {
            let mut state = self.reactive.borrow_mut();
            let Some((value, written)) = state.set_session_lock_state(next) else {
                return false;
            };
            if let Err(error) = written {
                state.log(LogLevel::Warn, format!("session lock state: {error}"));
            }
            value
        };
        if let Err(message) = self
            .lua
            .enter(|ctx| flush_reactive(&self.reactive, ctx, self.limits))
        {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("session lock binding: {message}"));
        }
        let callbacks = self
            .reactive
            .borrow()
            .session()
            .session_lock_callbacks
            .clone();
        for (callback, locked_only) in &callbacks {
            if *locked_only && next != SessionLockState::Locked {
                continue;
            }
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_handler_args(ctx, callback, std::slice::from_ref(&value), limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("session lock callback: {message}"));
            }
        }
        true
    }
}
