//! `morf.kit.native`: the widget archetypes of morf-kit, as a Lua module,
//! plus `own(id, node)`, which drops control `id` when `node` is removed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use luna::{Callback, CallbackReturn, Table, UserRef, Value as LuaValue};
use morf_kit::NativeKit;
use morf_scene::NodeHandle;

use crate::value::tokens::NodeToken;

/// Makes `morf.kit.native` available to every runtime made afterwards.
pub fn register() {
    crate::register_extension(install);
}

fn install(runtime: &mut crate::Runtime) {
    let kit = NativeKit::default();
    runtime.add_native_module("morf.kit.native", kit.functions());
    let owned: Rc<RefCell<HashMap<NodeHandle, Vec<i64>>>> = Rc::default();
    runtime.on_nodes_removed({
        let (kit, owned) = (kit.clone(), Rc::clone(&owned));
        Rc::new(move |removed| {
            let mut owned = owned.borrow_mut();
            for node in removed {
                for id in owned.remove(node).unwrap_or_default() {
                    kit.drop_control(id);
                }
            }
        })
    });
    runtime.lua.enter(|ctx| {
        let Ok(package) = ctx.get_global::<Table>("package") else {
            return;
        };
        let LuaValue::Table(loaded) = package.get_value(ctx, "loaded") else {
            return;
        };
        let LuaValue::Table(module) = loaded.get_value(ctx, "morf.kit.native") else {
            return;
        };
        let own = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (id, node): (i64, UserRef<NodeToken>) = stack.consume(ctx)?;
            let mut owned = owned.borrow_mut();
            let ids = owned.entry(node.handle).or_default();
            ids.retain(|id| kit.alive(*id));
            ids.push(id);
            Ok(CallbackReturn::Return)
        });
        module.set_field(ctx, "own", own);
    });
}
