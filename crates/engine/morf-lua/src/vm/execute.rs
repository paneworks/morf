use crate::states::Capture;
use luna::{Context, Executor, Fuel, Table, Value as LuaValue, Variadic};

use crate::vm::handler_store::stashed;
use morf_io::{DbusCall, DbusValue};
use morf_runtime::Handler;
use morf_scene::Value as SceneValue;
use morf_scene::reactive::EffectCapture;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use morf_system::{
    AuthMessageType, GreetdEvent, GreetdResponse, PamEvent, PamPrompt, StatusNotifierAddress,
    UdevEvent,
};

mod system;

pub(crate) use system::{
    execute_dbus_call_handler, execute_dbus_handler, execute_dbus_reply_handler,
    execute_dbus_signal_handler, execute_greetd_handler, execute_pam_session_handler,
    status_notifier_value, udev_event_value,
};

use crate::{
    reactive_bindings::*, scene_bindings::*, serialization::*, state::*, surface_types::*, types::*,
};

/// Runs one Lua executor to completion, or stops it when its fuel runs out.
///
/// This is the sandbox's only real defence: untrusted configuration code gets a
/// fixed number of VM instructions and is cut off at the end of them. It used
/// to be written out at every place that ran Lua — ten copies — and two of them
/// had already drifted: one skipped the error mapping every other copy applied,
/// and only one debited the per-frame budget, so a construction that ran a
/// factory once per item could spend a full effect budget on each of them.
///
/// Returns the fuel actually spent, so a caller that shares a budget across
/// several runs can debit it.
pub(crate) fn drive_executor<'gc>(
    ctx: Context<'gc>,
    executor: Executor<'gc>,
    limits: Limits,
    budget: u64,
    what: &str,
) -> Result<u64, String> {
    if budget == 0 {
        return Err(format!("Lua {what} fuel exhausted"));
    }
    let mut remaining = budget;
    loop {
        if remaining == 0 {
            executor.stop(&ctx);
            return Err(format!(
                "Lua {what} fuel exhausted after {budget} instructions"
            ));
        }
        let allowance = remaining.min(limits.slice_fuel.max(1) as u64) as i32;
        let mut fuel = Fuel::with(allowance);
        let finished = executor
            .step(ctx, &mut fuel)
            .map_err(|error| error.to_string())?;
        let consumed = allowance.saturating_sub(fuel.remaining()).max(0) as u64;
        remaining = remaining.saturating_sub(consumed.max(1));
        if finished {
            return Ok(budget - remaining);
        }
    }
}

pub(crate) fn evaluate_effect(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
    frame_remaining: &mut u64,
    token: u64,
    effect: &mut EffectCapture<IpcValue>,
) -> Result<(), String> {
    let lua_effect = {
        let mut state = state.borrow_mut();
        if state.active.is_some() {
            return Err("reactive effects cannot run recursively".to_owned());
        }
        // A binding whose node was removed during this flush: its effect
        // is waiting to be forgotten and has nothing left to drive.
        let Some(effect) = state.reactive.effects.get(&token).cloned() else {
            return Ok(());
        };
        state.active = Some(Capture::default());
        state.effect_runs = state.effect_runs.saturating_add(1);
        effect
    };
    let result = execute_effect(
        ctx,
        &lua_effect.handler,
        limits,
        frame_remaining,
        lua_effect.sink.is_some(),
    );
    let state_result = if let (Ok(Some(value)), Some(EffectSink::State(node))) =
        (&result, lua_effect.sink.clone())
    {
        match value {
            SceneValue::String(name) => {
                apply_state(state, ctx, limits, frame_remaining, node, name)
            }
            // No state chose itself and none is the default: the node stays
            // as it is.
            SceneValue::Nil => Ok(()),
            _ => Err("state binding must return a string".into()),
        }
    } else {
        Ok(())
    };
    let state_result = state_result.and_then(|()| {
        if let (Ok(Some(value)), Some(sink)) = (&result, lua_effect.sink) {
            match sink {
                EffectSink::Property(sink) => assign_scene_property(
                    &mut state.borrow_mut(),
                    sink.node,
                    &sink.property,
                    value.clone(),
                ),
                EffectSink::Loop(node) => morf_runtime::animation::loops::apply_loops(
                    &mut *state.borrow_mut(),
                    node,
                    value,
                ),
                EffectSink::State(_) => Ok(()),
            }
        } else {
            Ok(())
        }
    });
    let capture = state.borrow_mut().active.take().unwrap_or_default();
    for (node, property, target) in capture.property_reads {
        let key = (node, property.clone(), target);
        let signal = if let Some(signal) = state.borrow().property_signals.get(&key).copied() {
            signal
        } else {
            let name = format!("{node:?}.{property}{}", if target { "_target" } else { "" });
            let value = IpcValue::Integer(state.borrow().revisions.property_revision);
            let mut state = state.borrow_mut();
            let signal = state
                .reactive
                .graph
                .as_mut()
                .ok_or("reactive graph unavailable")?
                .signal(name.clone(), value.clone());
            state.property_signals.insert(key, signal);
            if !target {
                state.current_property_names.insert(name, (node, property));
            }
            state.reactive.values.insert(signal, value);
            state.reactive.signals.push(signal);
            signal
        };
        read_signal(state, effect, signal)?;
    }
    for signal in crate::model_revisions::model_read_signals(state, capture.model_reads)? {
        read_signal(state, effect, signal)?;
    }
    for signal in capture.reads {
        read_signal(state, effect, signal)?;
    }
    if result.is_ok() {
        for (signal, value) in capture.writes {
            let mut state = state.borrow_mut();
            effect
                .set(
                    state
                        .reactive
                        .graph
                        .as_ref()
                        .ok_or("reactive graph unavailable")?,
                    signal,
                    value.clone(),
                )
                .map_err(|error| error.to_string())?;
            state.reactive.values.insert(signal, value);
            state.shared.note_write(signal);
            state.reactive.flush_writes.push(signal);
        }
    }
    state_result?;
    result.map(|_| ())
}

/// Records that the effect being evaluated read `signal`.
fn read_signal(
    state: &Rc<RefCell<ReactiveState>>,
    effect: &mut EffectCapture<IpcValue>,
    signal: morf_scene::reactive::SignalId,
) -> Result<(), String> {
    let state = state.borrow();
    let graph = state
        .reactive
        .graph
        .as_ref()
        .ok_or("reactive graph unavailable")?;
    effect
        .get(graph, signal)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub(crate) fn execute_effect(
    ctx: Context<'_>,
    closure: &Handler,
    limits: Limits,
    frame_remaining: &mut u64,
    capture_value: bool,
) -> Result<Option<SceneValue>, String> {
    let budget = limits.effect_fuel.min(*frame_remaining);
    if budget == 0 {
        return Err("Lua frame fuel exhausted".to_owned());
    }
    let executor = Executor::start(ctx, ctx.fetch(&stashed(closure)).into(), ());
    match drive_executor(ctx, executor, limits, budget, "effect") {
        Err(error) => {
            *frame_remaining = frame_remaining.saturating_sub(budget);
            Err(error)
        }
        Ok(spent) => {
            *frame_remaining = frame_remaining.saturating_sub(spent);
            if capture_value {
                // Whatever a binding hands back goes to the property as it
                // is: a table is a gradient, a decoration, an anchor set.
                match executor.take_result::<LuaValue>(ctx) {
                    Ok(Ok(value)) => lua_to_scene(ctx, value, 0).map(Some),
                    Ok(Err(error)) => Err(error.to_string()),
                    Err(error) => Err(error.to_string()),
                }
            } else {
                match executor.take_result::<()>(ctx) {
                    Ok(Ok(())) => Ok(None),
                    Ok(Err(error)) => Err(error.to_string()),
                    Err(error) => Err(error.to_string()),
                }
            }
        }
    }
}

pub(crate) fn execute_handler_args(
    ctx: Context<'_>,
    closure: &Handler,
    args: &[IpcValue],
    limits: Limits,
) -> Result<(), String> {
    let args = Variadic(
        args.iter()
            .map(|value| value.to_lua(ctx))
            .collect::<Vec<_>>(),
    );
    let function = ctx.fetch(&stashed(closure));
    let _span =
        crate::profile::span(|| format!("handler {}", crate::profile::closure_origin(function)));
    let executor = Executor::start(ctx, function.into(), args);
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn execute_screencopy_handler(
    ctx: Context<'_>,
    closure: &Handler,
    result: Result<Screencopy, String>,
    limits: Limits,
) -> Result<(), String> {
    let args = match result {
        Ok(frame) => {
            let value = Table::new(&ctx);
            value.set_field(ctx, "width", i64::from(frame.width));
            value.set_field(ctx, "height", i64::from(frame.height));
            value.set_field(ctx, "stride", i64::from(frame.stride));
            value.set_field(ctx, "format", frame.format.as_str());
            value.set_field(ctx, "y_invert", frame.y_invert);
            value.set_field(ctx, "gpu", frame.gpu);
            value.set_field(ctx, "source", ctx.intern(frame.source.as_bytes()));
            value.set_field(ctx, "pixels", ctx.intern(&frame.pixels));
            Variadic(vec![LuaValue::Table(value), LuaValue::Nil])
        }
        Err(error) => Variadic(vec![
            LuaValue::Nil,
            LuaValue::String(ctx.intern(error.as_bytes())),
        ]),
    };
    let function = ctx.fetch(&stashed(closure));
    let _span =
        crate::profile::span(|| format!("handler {}", crate::profile::closure_origin(function)));
    let executor = Executor::start(ctx, function.into(), args);
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn execute_ipc_handler(
    ctx: Context<'_>,
    closure: &Handler,
    args: &[IpcValue],
    limits: Limits,
) -> Result<Vec<IpcValue>, String> {
    let args = Variadic(
        args.iter()
            .map(|value| value.to_lua(ctx))
            .collect::<Vec<_>>(),
    );
    let executor = Executor::start(ctx, ctx.fetch(&stashed(closure)).into(), args);
    drive_executor(ctx, executor, limits, limits.effect_fuel, "IPC handler")?;
    let values = match executor.take_result::<Variadic<Vec<LuaValue>>>(ctx) {
        Ok(Ok(values)) => values,
        Ok(Err(error)) => return Err(error.to_string()),
        Err(error) => return Err(error.to_string()),
    };
    // A table an IPC verb returns crosses the wire as JSON.
    values
        .into_iter()
        .map(|value| IpcValue::from_lua_deep(ctx, value))
        .collect()
}
