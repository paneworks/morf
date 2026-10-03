//! What a runner that is not a shell needs from a runtime: a clock it
//! advances itself, a command line of its own, and functions of its own for
//! Lua to call.
//!
//! `morf check`, `morf render` and `morf test` drive a configuration with no
//! compositor and no frame callbacks. Nothing here changes how a shell runs:
//! each is off until a runner asks for it.

use crate::ipc_table::{IpcFromLua, IpcToLua};
use luna::{Callback, CallbackReturn, Table, Value as LuaValue, Variadic};
use std::rc::Rc;
use std::time::Duration;

use crate::{scene_bindings::HostError, surface_types::IpcValue, types::Runtime};

/// A function a runner hands to Lua: primitive values and JSON-like tables
/// in, the same out, and an error raised in Lua as one.
pub type HostFunction = Rc<dyn Fn(Vec<IpcValue>) -> Result<Vec<IpcValue>, String>>;

impl Runtime {
    /// Moves every timer from the wall clock to a virtual one that stands at
    /// zero and moves only when [`Runtime::advance_virtual_clock`] says so.
    ///
    /// Called before the configuration runs, so every `morf.timer` and
    /// `ui.Timer` it makes is on that clock. Animations need nothing: they
    /// already advance by the delta [`Runtime::tick_animations`] is given.
    pub fn use_virtual_clock(&mut self) {
        let mut state = self.reactive.borrow_mut();
        if state.virtual_now.is_none() {
            state.virtual_now = Some(Duration::ZERO);
        }
    }

    /// The virtual clock's reading, or nothing when timers run off the wall.
    pub fn virtual_clock(&self) -> Option<Duration> {
        self.reactive.borrow().virtual_now
    }

    /// Moves the virtual clock forward. Timers that came due fire on the
    /// next [`Runtime::poll_services`], exactly as a wall timer's tick waits
    /// for the loop to come round.
    pub fn advance_virtual_clock(&mut self, by: Duration) {
        if let Some(now) = self.reactive.borrow_mut().virtual_now.as_mut() {
            *now += by;
        }
    }

    /// When the earliest virtual timer comes due, so a runner advancing a
    /// long way can stop at each one rather than stepping past it.
    pub fn next_virtual_deadline(&self) -> Option<Duration> {
        self.reactive
            .borrow()
            .timers
            .iter()
            .filter_map(|timer| timer.timer.deadline())
            .min()
    }

    /// Replaces `morf.args`, `morf.options` and `morf.operands` with another
    /// command line's, for a runner that loads configurations with arguments
    /// of their own rather than this process's.
    pub fn set_arguments(&mut self, words: Vec<String>) {
        let arguments = crate::arguments::Arguments::parse(words);
        self.lua.enter(|ctx| {
            let Ok(morf) = ctx.get_global::<Table>("morf") else {
                return;
            };
            crate::api_shell::set_argument_fields(ctx, morf, &arguments);
        });
    }

    /// Installs `table.name` as a global function that calls back into the
    /// host. The table is created when it does not exist yet.
    pub fn register_host_function(
        &mut self,
        table: &'static str,
        name: &'static str,
        function: HostFunction,
    ) {
        self.lua.enter(|ctx| {
            let holder = match ctx.get_global::<Table>(table) {
                Ok(existing) => existing,
                Err(_) => {
                    let created = Table::new(&ctx);
                    let _ = ctx.set_global(table, created);
                    created
                }
            };
            let callback = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let Variadic(arguments): Variadic<Vec<LuaValue>> = stack.consume(ctx)?;
                let arguments = arguments
                    .into_iter()
                    .map(|value| IpcValue::from_lua_deep(ctx, value))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(HostError)?;
                let results = function(arguments).map_err(HostError)?;
                let results = results
                    .iter()
                    .map(|value| value.to_lua(ctx))
                    .collect::<Vec<_>>();
                stack.replace(ctx, Variadic(results));
                Ok(CallbackReturn::Return)
            });
            holder.set_field(ctx, name, callback);
        });
    }
}
