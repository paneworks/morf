use luna::{Callback, CallbackReturn, Context, Table, UserData, UserRef, Value as LuaValue};
use std::rc::Rc;

use crate::{
    api::ui::bindings::*, api::ui::scene::*, runtime::helpers::*, runtime::host_types::*,
    runtime::state::*, vm::types::*,
};

pub(crate) fn install_signal_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    morf: Table<'gc>,
    limits: Limits,
) {
    let get = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let signal: UserRef<SignalToken> = stack.consume(ctx)?;
            let mut state = state.borrow_mut();
            let value = if let Some(active) = state.tracking_mut().capture() {
                active.reads.insert(signal.id);
                active
                    .writes
                    .iter()
                    .rev()
                    .find(|(id, _)| *id == signal.id)
                    .map(|(_, value)| value.clone())
                    .or_else(|| state.reactive().value(signal.id).cloned())
            } else {
                state.reactive().value(signal.id).cloned()
            }
            .ok_or_else(|| HostError("stale reactive signal".to_owned()))?;
            stack.replace(ctx, value.to_lua(ctx));
            Ok(CallbackReturn::Return)
        }
    });

    let set = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (signal, value): (UserRef<SignalToken>, LuaValue) = stack.consume(ctx)?;
            let value = IpcValue::from_lua_deep(ctx, value).map_err(HostError)?;
            {
                let mut state = state.borrow_mut();
                if let Some(active) = state.tracking_mut().capture() {
                    active.writes.push((signal.id, value));
                    stack.replace(ctx, true);
                    return Ok(CallbackReturn::Return);
                }
                state
                    .reactive_mut()
                    .write(signal.id, value)
                    .map_err(HostError)?;
                state.shared_mut().note_write(signal.id);
                // Inside a handler the write is enough: the graph is flushed
                // once, when the handler returns, however many writes it made.
                if state.tracking().in_handler() {
                    state.tracking_mut().defer_flush();
                    stack.replace(ctx, true);
                    return Ok(CallbackReturn::Return);
                }
            }
            replace_status(ctx, &mut stack, flush_status(&state, ctx, limits));
            Ok(CallbackReturn::Return)
        }
    });

    let methods = Table::new(&ctx);
    methods.set_field(ctx, "get", get);
    methods.set_field(ctx, "set", set);
    let signal_metatable = Table::new(&ctx);
    signal_metatable.set_field(ctx, "__index", methods);
    let signal_metatable = ctx.stash(signal_metatable);
    // A signal only its owner writes (a claim's presence): read as any
    // other, and a `set` refused.
    let read_only = Table::new(&ctx);
    read_only.set_field(ctx, "get", get);
    read_only.set_field(
        ctx,
        "set",
        Callback::from_fn(&ctx, |_, _, _| {
            Err(HostError("this signal is read-only: its owner moves it".to_owned()).into())
        }),
    );
    let read_only_metatable = Table::new(&ctx);
    read_only_metatable.set_field(ctx, "__index", read_only);
    state
        .borrow_mut()
        .metatables_mut()
        .set_read_only_signal(ctx.stash(read_only_metatable));

    let signal = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        let signal_metatable = signal_metatable.clone();
        move |ctx, _, mut stack| {
            let (name, value): (String, LuaValue) = stack.consume(ctx)?;
            let value = IpcValue::from_lua_deep(ctx, value).map_err(HostError)?;
            let id = {
                let mut state = state.borrow_mut();
                state
                    .reactive_mut()
                    .signal(name, value)
                    .ok_or_else(|| HostError(morf_runtime::reactive::GRAPH_RUNNING.to_owned()))?
            };
            let userdata = UserData::new_static(&ctx, SignalToken { id });
            userdata.set_metatable(ctx, Some(ctx.fetch(&signal_metatable)));
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        }
    });

    let reloadable = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        let signal_metatable = signal_metatable.clone();
        move |ctx, _, mut stack| {
            let (name, initial): (String, LuaValue) = stack.consume(ctx)?;
            let initial = IpcValue::from_lua_deep(ctx, initial).map_err(HostError)?;
            let (id, _) = register_reloadable_value(&mut state.borrow_mut(), name, initial)
                .map_err(HostError)?;
            let userdata = UserData::new_static(&ctx, SignalToken { id });
            userdata.set_metatable(ctx, Some(ctx.fetch(&signal_metatable)));
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        }
    });

    morf.set_field(ctx, "signal", signal);
    morf.set_field(ctx, "reloadable", reloadable);
    let shared = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        let signal_metatable = signal_metatable.clone();
        move |ctx, _, mut stack| {
            let (name, initial): (String, LuaValue) = stack.consume(ctx)?;
            let initial = IpcValue::from_lua_deep(ctx, initial).map_err(HostError)?;
            let id = crate::runtime::helpers::validate_scope_part(&name)
                .and_then(|()| {
                    let waker = morf_io::waker();
                    let ring: morf_runtime::shared::Ring =
                        std::sync::Arc::new(move || waker.wake());
                    state.borrow_mut().shared_signal(name, initial, ring)
                })
                .map_err(HostError)?;
            let userdata = UserData::new_static(&ctx, SignalToken { id });
            userdata.set_metatable(ctx, Some(ctx.fetch(&signal_metatable)));
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        }
    });
    morf.set_field(ctx, "shared", shared);
    crate::api::ui::channels::install(ctx, Rc::clone(&state), morf);
    let clock = UserData::new_static(
        &ctx,
        SignalToken {
            id: state.borrow().clocks().seconds,
        },
    );
    clock.set_metatable(ctx, Some(ctx.fetch(&signal_metatable)));
    crate::api::namespace::table(ctx, morf, "time").set_field(ctx, "clock", clock);
    // The same clock at coarser grains, for a binding that shows or checks
    // something by the minute or the hour: reading one of these instead of
    // `morf.time.clock` is what lets an idle shell sleep between them.
    for (name, id) in [
        ("minute_clock", state.borrow().clocks().minutes),
        ("hour_clock", state.borrow().clocks().hours),
    ] {
        let token = UserData::new_static(&ctx, SignalToken { id });
        token.set_metatable(ctx, Some(ctx.fetch(&signal_metatable)));
        crate::api::namespace::table(ctx, morf, "time").set_field(ctx, name, token);
    }
    // Where the session lock stands, for a binding to follow: "unlocked",
    // "pending", "locked" or "failed". Written by the lock loop only.
    let session_lock = UserData::new_static(
        &ctx,
        SignalToken {
            id: state.borrow().session().lock,
        },
    );
    session_lock.set_metatable(ctx, Some(ctx.fetch(&signal_metatable)));
    crate::api::namespace::table(ctx, morf, "lock").set_field(ctx, "state", session_lock);
}
