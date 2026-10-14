//! One flush per handler.
//!
//! A handler is any Lua the host runs in answer to something: a click, a
//! key, a timer, an IPC verb, a D-Bus call, a capture arriving. What the
//! handler writes -- signals, node properties -- is applied as it goes, and
//! the reactive graph is flushed once when the outermost handler returns.
//! Three signal writes are one flush; a bare `node.text = "x"` reaches the
//! bindings that read it before the next frame rather than at the next
//! unrelated write.

use luna::Context;

use crate::{api::ui::bindings::*, vm::types::*};

impl Runtime {
    /// Runs `body` in Lua as a handler, and flushes afterwards if it wrote.
    pub(crate) fn run_handler<T>(
        &mut self,
        body: impl for<'gc> FnOnce(Context<'gc>, Limits) -> T,
    ) -> T {
        self.reactive.borrow_mut().tracking.enter_handler();
        let limits = self.limits;
        let result = self.lua.enter(|ctx| body(ctx, limits));
        let flush = self.reactive.borrow_mut().tracking.leave_handler();
        let _span =
            flush.then(|| crate::profile::span(|| "engine: flush after a handler".to_owned()));
        if flush
            && let Err(message) = self
                .lua
                .enter(|ctx| flush_reactive(&self.reactive, ctx, self.limits))
        {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("after handler: {message}"));
        }
        // A handler that edited a text input — `:insert`, `:select` — owes
        // that field's callbacks, which could not run while it was running.
        self.drain_input_events();
        // What the handler removed — a model row gone, a node destroyed —
        // lets its hooks run now, with nothing borrowed.
        self.lua
            .enter(|ctx| run_destroyed_hooks(&self.reactive, ctx, self.limits));
        result
    }
}

/// The runtime's subsystems call handlers through this: each call is one
/// handler, with its flush, as any other the host runs.
impl morf_runtime::Handlers for Runtime {
    fn call(
        &mut self,
        handler: &morf_runtime::Handler,
        args: &[crate::IpcValue],
    ) -> Result<Vec<crate::IpcValue>, String> {
        self.run_handler(|ctx, limits| {
            crate::vm::execute::execute_ipc_handler(ctx, handler, args, limits)
        })
    }

    /// Whatever the handler returns is dropped unread, as a callback's is.
    fn notify(
        &mut self,
        handler: &morf_runtime::Handler,
        args: &[crate::IpcValue],
    ) -> Result<(), String> {
        self.run_handler(|ctx, limits| {
            crate::vm::execute::execute_handler_args(ctx, handler, args, limits)
        })
    }
}
