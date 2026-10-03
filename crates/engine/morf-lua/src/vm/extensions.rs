//! Native modules a host adds without the engine knowing them.
//!
//! A crate that builds on the engine -- `morf-kit`, the widget archetypes --
//! registers once, before any runtime is made, with
//! [`register_extension`]. Every [`Runtime`] made afterwards calls it, and
//! it adds its modules with [`Runtime::add_native_module`]: tables of host
//! functions that a configuration reaches with `require(name)`. A
//! configuration that never requires one pays nothing for it beyond the
//! table.

use crate::ipc_table::{IpcFromLua, IpcToLua};
use std::sync::Mutex;

use luna::{Callback, CallbackReturn, Table, Value as LuaValue, Variadic};

use crate::scene_bindings::HostError;
use crate::{HostFunction, IpcValue, Runtime};

/// What an extension does to each new runtime.
pub type Extension = fn(&mut Runtime);

static EXTENSIONS: Mutex<Vec<Extension>> = Mutex::new(Vec::new());

/// Adds `extension` to every runtime made from now on. Registering the same
/// one twice adds it once.
pub fn register_extension(extension: Extension) {
    let mut extensions = EXTENSIONS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if !extensions
        .iter()
        .any(|known| std::ptr::fn_addr_eq(*known, extension))
    {
        extensions.push(extension);
    }
}

pub(crate) fn apply_extensions(runtime: &mut Runtime) {
    let extensions = EXTENSIONS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    for extension in extensions {
        extension(runtime);
    }
}

impl Runtime {
    /// Makes `require(name)` return a table of `functions`, each called with
    /// its arguments as values and returning values.
    pub fn add_native_module(&mut self, name: &str, functions: Vec<(&'static str, HostFunction)>) {
        self.lua.enter(|ctx| {
            let module = Table::new(&ctx);
            for (field, function) in functions {
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
                module.set_field(ctx, field, callback);
            }
            if let Ok(package) = ctx.get_global::<Table>("package")
                && let LuaValue::Table(loaded) = package.get_value(ctx, "loaded")
            {
                let key = ctx.intern(name.as_bytes());
                let _ = loaded.set(ctx, key, module);
            }
        });
    }
}
