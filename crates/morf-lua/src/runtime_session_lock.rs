//! Where the session lock stands, as the compositor tells it.
//!
//! A lock client asks for the lock and the compositor answers later: it
//! confirms once every output shows a locked frame, or refuses, or ends the
//! lock of its own accord. The configuration hears each of these through
//! `morf.session_lock` (a signal), `morf.session_lock_state()`, and the
//! `morf.on_session_lock_state` / `morf.on_session_locked` callbacks.

use crate::{IpcValue, reactive_bindings::*, reactive_execute::*, state::*, types::*};

/// One step of an ext-session-lock-v1 lock's life.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionLockState {
    /// No lock is held by this process.
    Unlocked,
    /// The lock was asked for and the compositor has not answered yet.
    Pending,
    /// The compositor confirmed the lock (`locked`): the session is hidden.
    Locked,
    /// The compositor refused the lock (`finished` before `locked`).
    Failed,
}

impl SessionLockState {
    /// The name Lua sees.
    pub fn name(self) -> &'static str {
        match self {
            Self::Unlocked => "unlocked",
            Self::Pending => "pending",
            Self::Locked => "locked",
            Self::Failed => "failed",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        [Self::Unlocked, Self::Pending, Self::Locked, Self::Failed]
            .into_iter()
            .find(|state| state.name() == name)
    }
}

/// The state as last recorded.
pub(crate) fn current(state: &ReactiveState) -> SessionLockState {
    match state.values.get(&state.session_lock) {
        Some(IpcValue::String(name)) => {
            SessionLockState::parse(name).unwrap_or(SessionLockState::Unlocked)
        }
        _ => SessionLockState::Unlocked,
    }
}

impl Runtime {
    /// Where this process's session lock stands.
    pub fn session_lock_state(&self) -> SessionLockState {
        current(&self.reactive.borrow())
    }

    /// Records what the compositor said about the lock and tells the
    /// configuration, once per change. Returns whether anything changed.
    pub fn set_session_lock_state(&mut self, next: SessionLockState) -> bool {
        if self.session_lock_state() == next {
            return false;
        }
        let value = IpcValue::String(next.name().to_owned());
        {
            let mut state = self.reactive.borrow_mut();
            let signal = state.session_lock;
            if let Some(graph) = state.graph.as_mut()
                && let Err(error) = graph.write(signal, value.clone())
            {
                state.log(LogLevel::Warn, format!("session lock state: {error}"));
            }
            state.values.insert(signal, value.clone());
        }
        if let Err(message) = self
            .lua
            .enter(|ctx| flush_reactive(&self.reactive, ctx, self.limits))
        {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("session lock binding: {message}"));
        }
        let callbacks = self.reactive.borrow().session_lock_callbacks.clone();
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
