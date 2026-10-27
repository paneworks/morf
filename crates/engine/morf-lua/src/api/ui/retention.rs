use luna::{
    Callback, CallbackReturn, Closure, Context, Table, UserData, UserRef, Value as LuaValue,
};
use std::rc::Rc;

use crate::{api::ui::bindings::*, api::ui::scene::*, runtime::state::*, vm::types::*};

pub(crate) fn install_retention_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    morf: Table<'gc>,
    limits: Limits,
) {
    // `morf.effect(name, fn, { owner = node })` returns a handle whose
    // `:dispose()` takes the effect out of the graph; with an owner it also
    // goes when that node is removed. An effect made per panel build used
    // to outlive the panel and keep running for as long as the shell did.
    let effect_dispose = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let handle: UserRef<EffectHandleToken> = stack.consume(ctx)?;
            let disposed = dispose_effect(&mut state.borrow_mut(), handle.token);
            stack.replace(ctx, disposed);
            Ok(CallbackReturn::Return)
        }
    });
    let effect_alive = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let handle: UserRef<EffectHandleToken> = stack.consume(ctx)?;
            let alive = state.borrow().reactive().effect(handle.token).is_some();
            stack.replace(ctx, alive);
            Ok(CallbackReturn::Return)
        }
    });
    let effect_methods = Table::new(&ctx);
    effect_methods.set_field(ctx, "dispose", effect_dispose);
    effect_methods.set_field(ctx, "alive", effect_alive);
    let effect_metatable = Table::new(&ctx);
    effect_metatable.set_field(ctx, "__index", effect_methods);
    let effect_metatable = ctx.stash(effect_metatable);
    let effect = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (name, closure, options): (String, Closure, Option<Table>) = stack.consume(ctx)?;
            let owner = match options.map(|options| options.get_value(ctx, "owner")) {
                None | Some(LuaValue::Nil) => None,
                Some(LuaValue::UserData(node)) => Some(
                    node.downcast_static::<NodeToken>()
                        .map_err(|_| HostError("effect owner must be a node".to_owned()))?
                        .handle,
                ),
                Some(_) => return Err(HostError("effect owner must be a node".to_owned()).into()),
            };
            let token = {
                let mut state = state.borrow_mut();
                if let Some(owner) = owner
                    && !state.scene().contains(owner)
                {
                    return Err(HostError("effect owner is a removed node".to_owned()).into());
                }
                let token = state.tracking_mut().next_effect_token();
                state.reactive_mut().add_effect(
                    token,
                    LuaEffect {
                        handler: crate::vm::handler_store::register(ctx.stash(closure)),
                        sink: None,
                        owner,
                    },
                );
                // Inside another effect the graph is away: the new one is
                // queued and runs when that flush ends.
                state.register_external_effect(token, name);
                token
            };
            let handle = UserData::new_static(&ctx, EffectHandleToken { token });
            handle.set_metatable(ctx, Some(ctx.fetch(&effect_metatable)));
            // The handle on success, so `assert(morf.effect(...))` still
            // reads; `false, message, handle` when the first run failed.
            match flush_status(&state, ctx, limits) {
                Ok(()) => stack.replace(ctx, handle),
                Err(message) => stack.replace(ctx, (false, message, handle)),
            }
            Ok(CallbackReturn::Return)
        }
    });
    morf.set_field(ctx, "effect", effect);
}

/// What `morf.effect` returns: the effect's token, to dispose it by.
pub(crate) struct EffectHandleToken {
    pub(crate) token: u64,
}

/// Takes an effect out of the graph: it never runs again and depends on
/// nothing. While a flush holds the graph the removal waits for it to
/// finish. False if the effect was already gone.
pub(crate) fn dispose_effect(state: &mut ReactiveState, token: u64) -> bool {
    if !state.reactive_mut().dispose_effect(token) {
        return false;
    }
    state.collect_graph_garbage();
    true
}
