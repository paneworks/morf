use luna::{
    Callback, CallbackReturn, Closure, Context, Table, UserData, UserRef, Value as LuaValue,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::{
    reactive_bindings::*, runtime_helpers::*, scene_bindings::*, state::*, table_menu::*, types::*,
};

pub(crate) fn install_retention_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
    limits: Limits,
) {
    let retainable_lock = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let retainable: UserRef<RetainableToken> = stack.consume(ctx)?;
            let locks = state
                .borrow_mut()
                .retention
                .lock(retainable.node)
                .map_err(|error| HostError(error.to_string()))?;
            stack.replace(ctx, i64::from(locks));
            Ok(CallbackReturn::Return)
        }
    });
    let retainable_unlock = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let retainable: UserRef<RetainableToken> = stack.consume(ctx)?;
            let (locks, destroy) = {
                let mut state = state.borrow_mut();
                let locks = state
                    .retention
                    .unlock(retainable.node)
                    .map_err(|error| HostError(error.to_string()))?;
                let destroy = state
                    .retention
                    .should_destroy(retainable.node)
                    .unwrap_or(false);
                (locks, destroy)
            };
            if destroy {
                finish_retained_destroy(&state, ctx, limits, retainable.node);
            }
            stack.replace(ctx, i64::from(locks));
            Ok(CallbackReturn::Return)
        }
    });
    let retainable_force_unlock = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let retainable: UserRef<RetainableToken> = stack.consume(ctx)?;
            let destroy = {
                let mut state = state.borrow_mut();
                state
                    .retention
                    .force_unlock(retainable.node)
                    .map_err(|error| HostError(error.to_string()))?;
                state
                    .retention
                    .should_destroy(retainable.node)
                    .unwrap_or(false)
            };
            if destroy {
                finish_retained_destroy(&state, ctx, limits, retainable.node);
            }
            Ok(CallbackReturn::Return)
        }
    });
    let retainable_retained = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let retainable: UserRef<RetainableToken> = stack.consume(ctx)?;
            let retained = state
                .borrow()
                .retention
                .state(retainable.node)
                .is_some_and(|state| state.dropped);
            stack.replace(ctx, retained);
            Ok(CallbackReturn::Return)
        }
    });
    let retainable_locks = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let retainable: UserRef<RetainableToken> = stack.consume(ctx)?;
            let locks = state
                .borrow()
                .retention
                .state(retainable.node)
                .map_or(0, |state| state.locks);
            stack.replace(ctx, i64::from(locks));
            Ok(CallbackReturn::Return)
        }
    });
    let retainable_methods = Table::new(&ctx);
    retainable_methods.set_field(ctx, "lock", retainable_lock);
    retainable_methods.set_field(ctx, "unlock", retainable_unlock);
    retainable_methods.set_field(ctx, "force_unlock", retainable_force_unlock);
    retainable_methods.set_field(ctx, "retained", retainable_retained);
    retainable_methods.set_field(ctx, "locks", retainable_locks);
    let retainable_metatable = Table::new(&ctx);
    retainable_metatable.set_field(ctx, "__index", retainable_methods);
    let retainable_metatable = ctx.stash(retainable_metatable);
    let retainable = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        let retainable_metatable = retainable_metatable.clone();
        move |ctx, _, mut stack| {
            let (node, options): (UserRef<NodeToken>, LuaValue) = stack.consume(ctx)?;
            state
                .borrow()
                .scene
                .element(node.handle)
                .map_err(|error| HostError(error.to_string()))?;
            let mut callbacks = RetainCallbacks::default();
            let mut locked = false;
            match options {
                LuaValue::Nil => {}
                LuaValue::Table(options) => {
                    locked = table_bool(ctx, options, "locked", false).map_err(HostError)?;
                    callbacks.dropped =
                        optional_closure(ctx, options, "on_dropped").map_err(HostError)?;
                    callbacks.about_to_destroy =
                        optional_closure(ctx, options, "on_about_to_destroy").map_err(HostError)?;
                }
                _ => {
                    return Err(
                        HostError("retainable options must be a table or nil".into()).into(),
                    );
                }
            }
            {
                let mut state = state.borrow_mut();
                state.retention.register(node.handle);
                if locked {
                    state
                        .retention
                        .lock(node.handle)
                        .map_err(|error| HostError(error.to_string()))?;
                }
                state.retain_callbacks.insert(node.handle, callbacks);
            }
            let userdata = UserData::new_static(&ctx, RetainableToken { node: node.handle });
            userdata.set_metatable(ctx, Some(ctx.fetch(&retainable_metatable)));
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        }
    });

    let retain_lock_locked = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let lock: UserRef<RetainLockToken> = stack.consume(ctx)?;
        stack.replace(ctx, lock.locked.get());
        Ok(CallbackReturn::Return)
    });
    let retain_lock_set = Callback::from_fn(&ctx, {
        move |ctx, _, mut stack| {
            let (lock, locked): (UserRef<RetainLockToken>, bool) = stack.consume(ctx)?;
            if lock.locked.get() == locked {
                return Ok(CallbackReturn::Return);
            }
            let destroy = {
                let mut state = lock.state.borrow_mut();
                if locked {
                    state
                        .retention
                        .lock(lock.node)
                        .map_err(|error| HostError(error.to_string()))?;
                    false
                } else {
                    state
                        .retention
                        .unlock(lock.node)
                        .map_err(|error| HostError(error.to_string()))?;
                    state.retention.should_destroy(lock.node).unwrap_or(false)
                }
            };
            lock.locked.set(locked);
            if destroy {
                finish_retained_destroy(&lock.state, ctx, limits, lock.node);
            }
            Ok(CallbackReturn::Return)
        }
    });
    let retain_lock_retained = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let lock: UserRef<RetainLockToken> = stack.consume(ctx)?;
        let retained = lock
            .state
            .borrow()
            .retention
            .state(lock.node)
            .is_some_and(|state| state.dropped);
        stack.replace(ctx, retained);
        Ok(CallbackReturn::Return)
    });
    let retain_lock_methods = Table::new(&ctx);
    retain_lock_methods.set_field(ctx, "locked", retain_lock_locked);
    retain_lock_methods.set_field(ctx, "set_locked", retain_lock_set);
    retain_lock_methods.set_field(ctx, "retained", retain_lock_retained);
    let retain_lock_metatable = Table::new(&ctx);
    retain_lock_metatable.set_field(ctx, "__index", retain_lock_methods);
    let retain_lock_metatable = ctx.stash(retain_lock_metatable);
    let retain_lock = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (retainable, locked): (UserRef<RetainableToken>, LuaValue) = stack.consume(ctx)?;
            let locked = match locked {
                LuaValue::Nil => true,
                LuaValue::Boolean(locked) => locked,
                _ => return Err(HostError("retain lock state must be boolean".into()).into()),
            };
            if locked {
                state
                    .borrow_mut()
                    .retention
                    .lock(retainable.node)
                    .map_err(|error| HostError(error.to_string()))?;
            }
            let userdata = UserData::new_static(
                &ctx,
                RetainLockToken {
                    node: retainable.node,
                    locked: Cell::new(locked),
                    state: Rc::clone(&state),
                },
            );
            userdata.set_metatable(ctx, Some(ctx.fetch(&retain_lock_metatable)));
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        }
    });

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
            let alive = state.borrow().reactive.effects.contains_key(&handle.token);
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
                    && !state.scene.contains(owner)
                {
                    return Err(HostError("effect owner is a removed node".to_owned()).into());
                }
                let token = state.next_effect;
                state.next_effect = state.next_effect.wrapping_add(1);
                state.reactive.effects.insert(
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
            match flush_reactive(&state, ctx, limits) {
                Ok(()) => stack.replace(ctx, handle),
                Err(message) => stack.replace(ctx, (false, message, handle)),
            }
            Ok(CallbackReturn::Return)
        }
    });
    morf.set_field(ctx, "retainable", retainable);
    morf.set_field(ctx, "retain_lock", retain_lock);
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
    if state.reactive.effects.remove(&token).is_none() {
        return false;
    }
    if let Some(id) = state.reactive.effect_ids.remove(&token) {
        state.reactive.dead_effects.push(id);
    }
    state.collect_graph_garbage();
    true
}
