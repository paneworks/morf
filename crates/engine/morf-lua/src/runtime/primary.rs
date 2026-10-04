//! The primary runtime: the one of a process that does what must be done
//! once.
//!
//! A shell runs its configuration once per output, each run a runtime of its
//! own, and some duties belong to the process rather than to a screen: a bus
//! name only one connection can own (a notification server, a tray watcher),
//! a history file only one writer should keep. The host says which runtime
//! is primary -- exactly one at a time -- and the configuration hears it
//! through `morf.primary()` (tracked) and `morf.on_primary(fn(is_primary))`.
//!
//! When the duty moves, the runtime giving it up gives its bus names back
//! before the next one is told ([`Runtime::release_bus_names`], which every
//! runtime also does when it is dropped), so the new primary finds the names
//! free.

use crate::{reactive_bindings::*, reactive_execute::*, types::*};

impl Runtime {
    /// Whether this runtime is the primary one. A runtime nobody told
    /// otherwise is.
    pub fn is_primary(&self) -> bool {
        self.reactive.borrow().session.is_primary()
    }

    /// Makes this runtime the primary one, or not: `morf.primary()` follows
    /// and the `morf.on_primary` callbacks hear the new value, once per
    /// change. Returns whether anything changed.
    ///
    /// Set before the configuration runs, it is simply what the configuration
    /// sees from its first line, and no callback is owed.
    pub fn set_primary(&mut self, primary: bool) -> bool {
        let value = {
            let mut state = self.reactive.borrow_mut();
            let state = &mut *state;
            let Some((value, written)) = state.session.set_primary(&mut state.reactive, primary)
            else {
                return false;
            };
            if let Err(error) = written {
                state.log(LogLevel::Warn, format!("primary: {error}"));
            }
            value
        };
        if let Err(message) = self
            .lua
            .enter(|ctx| flush_reactive(&self.reactive, ctx, self.limits))
        {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("primary binding: {message}"));
        }
        let callbacks = self.reactive.borrow().session.primary_callbacks.clone();
        for callback in &callbacks {
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_handler_args(ctx, callback, std::slice::from_ref(&value), limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("primary callback: {message}"));
            }
        }
        true
    }

    /// Gives back every bus name `morf.dbus.serve` took, now, rather than
    /// when the collector reaches the handles. Each release is a call the bus
    /// answers, so once this returns the names are free for another
    /// connection to take. The services stay open, answering nothing new.
    pub fn release_bus_names(&mut self) {
        let names = std::mem::take(&mut self.reactive.borrow_mut().owned_bus_names);
        for service in names.iter().filter_map(std::rc::Weak::upgrade) {
            service.borrow_mut().release();
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.release_bus_names();
    }
}
