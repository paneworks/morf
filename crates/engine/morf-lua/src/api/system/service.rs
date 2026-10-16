//! `morf.service(name, options)`: a system service the library's registry
//! describes (`library/lib/services/registry.lua`), bound as reactive state.
//!
//! The registry's binder does the binding; this is the entry point a
//! configuration calls. It is gated under the `services` permission group,
//! like `morf.run` under `run`, and `options.owner` ties the binding to a
//! node: removing the node closes the binding, after the node's own
//! `on_destroyed`. Everything the binding holds -- subscriptions, timers --
//! belongs to the runtime as well, so a runtime that ends ends them too.

use luna::{Callback, CallbackReturn, Closure, Function, Table, UserRef, Value as LuaValue};
use std::rc::Rc;

use crate::{api::ui::scene::*, runtime::state::*};

/// Binds through the registry, and hands the binding's `close` to the owner
/// node, taking it back when the binding is closed first.
const BIND: &[u8] = br#"
local name, options, native = ...
local registry = require("lib.services.registry")
local _, missing = registry.find(name)
if missing then error(missing, 0) end
local binding = registry.connect(name, options)
local owner = type(options) == "table" and options.owner or nil
if owner ~= nil then
  local id = native.own(owner, binding.close)
  binding.own(function() native.disown(id) end)
end
return binding
"#;

pub(crate) fn install_service_api<'gc>(
    ctx: luna::Context<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let own = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (node, close): (UserRef<NodeToken>, Closure) = stack.consume(ctx)?;
            let hook = crate::vm::handler_store::register(ctx.stash(close));
            let id = state
                .borrow_mut()
                .destroy_hooks_mut()
                .own(node.handle, hook);
            stack.replace(ctx, id as i64);
            Ok(CallbackReturn::Return)
        }
    });
    let disown = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let id: i64 = stack.consume(ctx)?;
            let gone = state.borrow_mut().destroy_hooks_mut().disown(id as u64);
            stack.replace(ctx, gone);
            Ok(CallbackReturn::Return)
        }
    });
    let native = Table::new(&ctx);
    native.set_field(ctx, "own", own);
    native.set_field(ctx, "disown", disown);
    let native = ctx.stash(native);
    let bind = match Closure::load(ctx, Some("morf.service"), BIND) {
        Ok(closure) => ctx.stash(closure),
        Err(error) => panic!("morf.service's binder does not compile: {error}"),
    };
    let service = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (name, options): (LuaValue, LuaValue) = stack.consume(ctx)?;
        if let Some(refusal) = state.borrow().permission_refusal("services") {
            stack.replace(ctx, (LuaValue::Nil, refusal.as_str()));
            return Ok(CallbackReturn::Return);
        }
        if let LuaValue::Table(options) = options {
            match options.get_value(ctx, "owner") {
                LuaValue::Nil => {}
                LuaValue::UserData(node) => {
                    let handle = node
                        .downcast_static::<NodeToken>()
                        .map_err(|_| HostError("service owner must be a node".to_owned()))?
                        .handle;
                    if !state.borrow().scene().contains(handle) {
                        return Err(HostError("service owner is a removed node".to_owned()).into());
                    }
                }
                _ => return Err(HostError("service owner must be a node".to_owned()).into()),
            }
        }
        stack.replace(ctx, (name, options, LuaValue::Table(ctx.fetch(&native))));
        Ok(CallbackReturn::Call {
            function: Function::Closure(ctx.fetch(&bind)),
            then: None,
        })
    });
    morf.set_field(ctx, "service", service);
}
