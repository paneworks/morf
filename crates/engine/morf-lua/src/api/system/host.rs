//! The host's services for a configuration: idle, output power, keyboard
//! focus, the backdrop, IPC handlers, the screens and the primary runtime.

use luna::{Callback, CallbackReturn, Closure, Context, Function, Table, Value as LuaValue};
use std::cell::RefCell;
use std::rc::Rc;

use crate::{scene_bindings::*, state::*, surface_types::*, types::*};

mod text_entry;

pub(crate) fn install_host_service_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
    screen: Option<&Screen>,
) {
    let idle_state = Rc::clone(&state);
    let idle_subscribe = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        // The third argument asks for *input* idleness: time since the person
        // last touched anything, ignoring idle inhibitors. A media player
        // inhibits idle so the screen stays on, and a shell that dims its own
        // bar after a minute of no input still wants to know about the minute.
        let (milliseconds, callback, input_only): (i64, Closure, Option<bool>) =
            stack.consume(ctx)?;
        let milliseconds = u32::try_from(milliseconds)
            .map_err(|_| HostError("idle timeout must fit an unsigned 32-bit value".into()))?;
        let key = (milliseconds, input_only.unwrap_or(false));
        let mut state = idle_state.borrow_mut();
        let callback_count = state
            .requests
            .idle_callbacks
            .values()
            .map(Vec::len)
            .sum::<usize>();
        if callback_count >= 256 {
            return Err(HostError("idle callback limit reached".into()).into());
        }
        if !state.requests.idle_callbacks.contains_key(&key)
            && state.requests.idle_callbacks.len() >= 64
        {
            return Err(HostError("idle timeout limit reached".into()).into());
        }
        let id = state.requests.next_idle_subscription;
        state.requests.next_idle_subscription += 1;
        // A threshold the compositor has not been asked for yet.
        state.requests.idle_timeouts_changed |= !state.requests.idle_callbacks.contains_key(&key);
        state
            .requests
            .idle_callbacks
            .entry(key)
            .or_default()
            .push((id, crate::vm::handler_store::register(ctx.stash(callback))));
        drop(state);
        // The subscription, so it can be let go: `sub:cancel()`. The last
        // callback on a threshold takes the compositor's notification with it.
        let cancel_state = Rc::clone(&idle_state);
        let cancel = Callback::from_fn(&ctx, move |_, _, _| {
            let mut state = cancel_state.borrow_mut();
            if let Some(callbacks) = state.requests.idle_callbacks.get_mut(&key) {
                callbacks.retain(|(held, _)| *held != id);
                if callbacks.is_empty() {
                    state.requests.idle_callbacks.remove(&key);
                    state.requests.idle_timeouts_changed = true;
                }
            }
            Ok(CallbackReturn::Return)
        });
        let subscription = Table::new(&ctx);
        subscription.set_field(ctx, "cancel", cancel);
        stack.replace(ctx, subscription);
        Ok(CallbackReturn::Return)
    });
    let inhibit_state = Rc::clone(&state);
    // The other direction from `subscribe`: not "tell me when the session goes
    // idle" but "do not let it". A video player, a presentation, a long copy —
    // each wants the screen to stay awake while nobody touches the input.
    let idle_inhibit = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let inhibited: bool = stack.consume(ctx)?;
        inhibit_state
            .borrow_mut()
            .requests
            .set_idle_inhibited(inhibited);
        Ok(CallbackReturn::Return)
    });
    // What was last asked for: whether this shell is keeping the session
    // awake. Whether the compositor can is `morf.capabilities.idle_inhibit`.
    let inhibited_state = Rc::clone(&state);
    let idle_inhibited = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        stack.replace(ctx, inhibited_state.borrow().requests.idle_inhibited);
        Ok(CallbackReturn::Return)
    });
    let idle = Table::new(&ctx);
    idle.set_field(ctx, "subscribe", idle_subscribe);
    idle.set_field(ctx, "inhibit", idle_inhibit);
    idle.set_field(ctx, "inhibited", idle_inhibited);
    morf.set_field(ctx, "idle", idle);
    let output_power_state = Rc::clone(&state);
    let output_power_set = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let mode: String = stack.consume(ctx)?;
        let on = match mode.as_str() {
            "off" => false,
            "on" => true,
            _ => return Err(HostError("output power mode must be `on` or `off`".into()).into()),
        };
        let mut state = output_power_state.borrow_mut();
        state.requests.queue_output_power(on).map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let output_power = Table::new(&ctx);
    output_power.set_field(ctx, "set", output_power_set);
    morf.set_field(ctx, "output_power", output_power);
    crate::api_gamma::install_gamma_api(ctx, Rc::clone(&state), morf);
    crate::api_clipboard::install_clipboard_api(ctx, Rc::clone(&state), morf);
    // `morf.on_keyboard_focus(function(active) end)`: the keyboard came to
    // the shell's surface, or left it. With `keyboard_focus = "on_demand"`
    // a click anywhere else is what takes it away.
    let keyboard_focus_state = Rc::clone(&state);
    let on_keyboard_focus = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let callback: Closure = stack.consume(ctx)?;
        let mut state = keyboard_focus_state.borrow_mut();
        if state.requests.keyboard_focus_callbacks.len() >= 64 {
            return Err(HostError("keyboard focus callback limit reached".into()).into());
        }
        state
            .requests
            .keyboard_focus_callbacks
            .push(crate::vm::handler_store::register(ctx.stash(callback)));
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "on_keyboard_focus", on_keyboard_focus);
    // `morf.on_backdrop_click(function() end)`: a click anywhere on the
    // output but the shell's surface, while `morf.surface.backdrop` is true.
    let backdrop_state = Rc::clone(&state);
    let on_backdrop_click = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let callback: Closure = stack.consume(ctx)?;
        let mut state = backdrop_state.borrow_mut();
        if state.requests.backdrop_callbacks.len() >= 64 {
            return Err(HostError("backdrop callback limit reached".into()).into());
        }
        state
            .requests
            .backdrop_callbacks
            .push(crate::vm::handler_store::register(ctx.stash(callback)));
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "on_backdrop_click", on_backdrop_click);
    crate::api_screencopy::install_screencopy_api(ctx, Rc::clone(&state), morf);
    text_entry::install_text_entry(ctx, Rc::clone(&state), morf);
    crate::api_time::install_timer_api(ctx, Rc::clone(&state), morf);
    let ipc_register = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (_table, name, value): (Table, String, LuaValue) = stack.consume(ctx)?;
            match value {
                LuaValue::Function(Function::Closure(closure)) => {
                    state
                        .borrow_mut()
                        .ipc_handlers
                        .insert(name, crate::vm::handler_store::register(ctx.stash(closure)));
                }
                LuaValue::Nil => {
                    state.borrow_mut().ipc_handlers.remove(&name);
                }
                _ => {
                    return Err(
                        HostError("morf.ipc values must be functions or nil".to_owned()).into(),
                    );
                }
            }
            Ok(CallbackReturn::Return)
        }
    });
    let ipc_metatable = Table::new(&ctx);
    ipc_metatable.set_field(ctx, "__newindex", ipc_register);
    let ipc = Table::new(&ctx);
    ipc.set_metatable(ctx, Some(ipc_metatable));
    morf.set_field(ctx, "ipc", ipc);
    let screens = Table::new(&ctx);
    // `morf.screens` is ordered: index 1 is always the output this
    // configuration instance drives, and `Runtime::set_screens` appends the
    // compositor's remaining outputs after it.
    if let Some(screen) = screen {
        screens
            .set(ctx, 1, screen_entry(ctx, screen))
            .expect("screen table accepts integer keys");
    }
    morf.set_field(ctx, "screens", screens);
    // `morf.screens` is a plain table refreshed in place; this is the tracked
    // read beside it, so a binding or an effect runs again when an output
    // comes, goes, moves or rescales -- instead of a timer comparing lists.
    {
        let mut state = state.borrow_mut();
        let revision = state
            .reactive
            .graph
            .as_mut()
            .expect("the graph is not running at install")
            .signal("screens.revision", IpcValue::Integer(0));
        state.reactive.values.insert(revision, IpcValue::Integer(0));
        state.reactive.signals.push(revision);
        state.screens_revision = Some((revision, 0));
        state.screens_signature = screen.map(screens_signature_of_one).unwrap_or_default();
    }
    let revision_state = Rc::clone(&state);
    morf.set_field(
        ctx,
        "screens_revision",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let mut state = revision_state.borrow_mut();
            let Some((signal, count)) = state.screens_revision else {
                stack.replace(ctx, 0);
                return Ok(CallbackReturn::Return);
            };
            if let Some(active) = &mut state.active {
                active.reads.insert(signal);
            }
            stack.replace(ctx, count);
            Ok(CallbackReturn::Return)
        }),
    );

    // `morf.primary()`: whether this runtime is the one of the process that
    // does what must be done once (a bus name, a shared file). Tracked, so a
    // binding or an effect runs again when the duty moves here or away; a
    // runtime nobody else shares a process with is primary.
    {
        let mut state = state.borrow_mut();
        let signal = state
            .reactive
            .graph
            .as_mut()
            .expect("the graph is not running at install")
            .signal("primary", IpcValue::Boolean(true));
        state
            .reactive
            .values
            .insert(signal, IpcValue::Boolean(true));
        state.reactive.signals.push(signal);
        state.primary = Some((signal, true));
    }
    let primary_state = Rc::clone(&state);
    morf.set_field(
        ctx,
        "primary",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let mut state = primary_state.borrow_mut();
            let Some((signal, primary)) = state.primary else {
                stack.replace(ctx, true);
                return Ok(CallbackReturn::Return);
            };
            if let Some(active) = &mut state.active {
                active.reads.insert(signal);
            }
            stack.replace(ctx, primary);
            Ok(CallbackReturn::Return)
        }),
    );
    let on_primary_state = Rc::clone(&state);
    morf.set_field(
        ctx,
        "on_primary",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let callback: Closure = stack.consume(ctx)?;
            let mut state = on_primary_state.borrow_mut();
            if state.primary_callbacks.len() >= 64 {
                return Err(HostError("primary callback limit reached".into()).into());
            }
            state
                .primary_callbacks
                .push(crate::vm::handler_store::register(ctx.stash(callback)));
            Ok(CallbackReturn::Return)
        }),
    );

    // Every window the compositor reports, filled by `Runtime::set_windows` and
    // updated in place. Empty here rather than absent so a configuration can
    // hold it and watch it from the first line, before any compositor has said
    // anything — and so `#morf.windows` is a number rather than an error on a
    // compositor that does not report them at all.
    let windows = Table::new(&ctx);
    morf.set_field(ctx, "windows", windows);
    // Filled once the compositor connection is up; empty rather than absent so
    // a configuration can index it from its first line.
    morf.set_field(ctx, "capabilities", Table::new(&ctx));
    crate::api_compositor::install_compositor_api(ctx, Rc::clone(&state), morf);

    // Empty rather than absent, so a configuration can hold it and watch it
    // from its first line — and so `#morf.workspaces` is a number rather than
    // an error on a compositor that does not speak the protocol at all.
}

/// Builds the Lua table describing one output.
pub(crate) fn screen_entry<'gc>(ctx: Context<'gc>, screen: &Screen) -> Table<'gc> {
    let value = Table::new(&ctx);
    value.set_field(ctx, "id", screen.id as i64);
    value.set_field(ctx, "name", screen.name.as_str());
    value.set_field(ctx, "make", screen.make.as_str());
    value.set_field(ctx, "model", screen.model.as_str());
    value.set_field(
        ctx,
        "description",
        screen
            .description
            .as_deref()
            .map_or(LuaValue::Nil, |description| {
                LuaValue::String(ctx.intern(description.as_bytes()))
            }),
    );
    value.set_field(
        ctx,
        "x",
        screen.position.map_or(LuaValue::Nil, |position| {
            LuaValue::Integer(position.0 as i64)
        }),
    );
    value.set_field(
        ctx,
        "y",
        screen.position.map_or(LuaValue::Nil, |position| {
            LuaValue::Integer(position.1 as i64)
        }),
    );
    value.set_field(
        ctx,
        "width",
        screen
            .width
            .map_or(LuaValue::Nil, |value| LuaValue::Integer(value as i64)),
    );
    value.set_field(
        ctx,
        "height",
        screen
            .height
            .map_or(LuaValue::Nil, |value| LuaValue::Integer(value as i64)),
    );
    value.set_field(ctx, "scale", screen.scale as i64);
    value.set_field(ctx, "device_pixel_ratio", screen.scale as i64);
    value.set_field(ctx, "transform", screen.transform.as_str());
    let physical_width = screen.physical_size.map(|size| size.0);
    let physical_height = screen.physical_size.map(|size| size.1);
    value.set_field(
        ctx,
        "physical_width_mm",
        physical_width.map_or(LuaValue::Nil, |value| LuaValue::Integer(value as i64)),
    );
    value.set_field(
        ctx,
        "physical_height_mm",
        physical_height.map_or(LuaValue::Nil, |value| LuaValue::Integer(value as i64)),
    );
    let physical_density = screen_density(screen);
    value.set_field(
        ctx,
        "physical_pixel_density",
        physical_density.map_or(LuaValue::Nil, LuaValue::Number),
    );
    value.set_field(
        ctx,
        "logical_pixel_density",
        physical_density.map_or(LuaValue::Nil, |density| {
            LuaValue::Number(density / f64::from(screen.scale.max(1)))
        }),
    );
    value.set_field(ctx, "orientation", screen_orientation(screen));
    value.set_field(
        ctx,
        "primary_orientation",
        screen_primary_orientation(screen),
    );
    value.set_field(ctx, "serial_number", LuaValue::Nil);
    value
}

/// What makes two output lists the same for `morf.screens_revision`.
pub(crate) fn screens_signature(screens: &[Screen]) -> String {
    screens
        .iter()
        .map(screens_signature_of_one)
        .collect::<Vec<_>>()
        .join(";")
}

fn screens_signature_of_one(screen: &Screen) -> String {
    format!(
        "{}|{:?}|{:?}x{:?}|{}|{}",
        screen.name, screen.position, screen.width, screen.height, screen.scale, screen.transform
    )
}
