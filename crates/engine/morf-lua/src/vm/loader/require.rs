//! `require`: the native modules, then `package.loaded`, then the runtime
//! path. Each module's top level runs inside the permission gate's scope
//! for it, held back when it is an unapproved imported module.

use luna::{Callback, CallbackReturn, Closure, Context, StashedTable, Value as LuaValue};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use super::{resolve_runtime_module, run_module};
use crate::api::ui::scene::HostError;
use crate::runtime::state::{ReactiveState, StateCell};
use crate::vm::permissions::{ModuleKind, Permissions};
use crate::vm::types::Limits;

pub(crate) struct Require {
    pub(crate) morf: StashedTable,
    pub(crate) ui: StashedTable,
    pub(crate) loaded: StashedTable,
    pub(crate) state: Rc<StateCell<ReactiveState>>,
    pub(crate) roots: Rc<RefCell<Vec<PathBuf>>>,
    pub(crate) permissions: Rc<RefCell<Permissions>>,
    pub(crate) limits: Limits,
}

pub(crate) fn install_require<'gc>(ctx: Context<'gc>, require: Require) {
    ctx.set_global(
        "require",
        Callback::from_fn(&ctx, move |ctx, exec, mut stack| {
            let name: String = stack.consume(ctx)?;
            // A missing or failing module is blamed on the line that asked
            // for it.
            let located = |error: String| match exec.frame_at(0) {
                Some(frame) => format!(
                    "{}:{}: {error}",
                    frame.chunk_name.display_lossy(),
                    frame.current_line
                ),
                None => error,
            };
            match name.as_str() {
                "morf" => stack.replace(ctx, ctx.fetch(&require.morf)),
                "morf.ui" => stack.replace(ctx, ctx.fetch(&require.ui)),
                _ => {
                    let loaded = ctx.fetch(&require.loaded);
                    let key = ctx.intern(name.as_bytes());
                    let cached = loaded.get_value(ctx, key);
                    if !matches!(cached, LuaValue::Nil) {
                        stack.replace(ctx, cached);
                        return Ok(CallbackReturn::Return);
                    }
                    loaded.set(ctx, key, true)?;
                    match load(ctx, &require, &name) {
                        Ok(module) => {
                            loaded.set(ctx, key, module)?;
                            stack.replace(ctx, module);
                        }
                        Err((error, at_call)) => {
                            loaded.set(ctx, key, LuaValue::Nil)?;
                            let error = if at_call { located(error) } else { error };
                            return Err(HostError(error).into());
                        }
                    }
                }
            }
            Ok(CallbackReturn::Return)
        }),
    );
}

/// Resolves, admits and runs one module; an error says whether it belongs
/// to the `require` call (not found) or to the module itself.
fn load<'gc>(
    ctx: Context<'gc>,
    require: &Require,
    name: &str,
) -> Result<LuaValue<'gc>, (String, bool)> {
    let roots = require.roots.borrow().clone();
    let (path, source) = resolve_runtime_module(&roots, name).map_err(|error| (error, true))?;
    let closure =
        Closure::load(ctx, Some(name), &source).map_err(|error| (error.to_string(), false))?;
    let holder = require.permissions.borrow_mut().admit(
        ctx,
        &require.state,
        &roots,
        name,
        &path,
        ModuleKind::Module,
    );
    require.state.borrow_mut().permissions_mut().enter(holder);
    let result = run_module(ctx, closure, require.limits);
    require.state.borrow_mut().permissions_mut().leave();
    match result {
        Ok(LuaValue::Nil) => Ok(LuaValue::Boolean(true)),
        Ok(module) => Ok(module),
        Err(error) => Err((error, false)),
    }
}
