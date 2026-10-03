//! `morf.log(level, ...)`: a configuration's own lines in the shell's log.
//!
//! The same log the engine writes its warnings to, read with `morf ipc log`,
//! stamped and capped the same way. `level` is `debug`, `info`, `warn` or
//! `error`; the rest are joined with spaces through `tostring`, as `print`
//! joins them. `morf.log.info(...)` and its siblings are the same call.

use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};
use std::cell::RefCell;
use std::rc::Rc;

use crate::{scene_bindings::*, state::*, types::*};

fn level_of(name: &[u8]) -> Option<LogLevel> {
    match name {
        b"debug" => Some(LogLevel::Debug),
        b"info" => Some(LogLevel::Info),
        b"warn" | b"warning" => Some(LogLevel::Warn),
        b"error" => Some(LogLevel::Error),
        _ => None,
    }
}

fn joined(values: impl Iterator<Item = String>) -> String {
    values.collect::<Vec<_>>().join(" ")
}

fn text_of(value: LuaValue<'_>) -> String {
    match value {
        LuaValue::String(text) => text.display_lossy().to_string(),
        LuaValue::Nil => "nil".to_owned(),
        LuaValue::Boolean(value) => value.to_string(),
        LuaValue::Integer(value) => value.to_string(),
        LuaValue::Number(value) => value.to_string(),
        other => format!("<{}>", other.type_name()),
    }
}

pub(crate) fn install_log_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let log = Table::new(&ctx);
    for (name, level) in [
        ("debug", LogLevel::Debug),
        ("info", LogLevel::Info),
        ("warn", LogLevel::Warn),
        ("error", LogLevel::Error),
    ] {
        let state = Rc::clone(&state);
        log.set_field(
            ctx,
            name,
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let message = joined((0..stack.len()).map(|index| text_of(stack.get(index))));
                state.borrow_mut().log(level, message);
                stack.clear();
                let _ = ctx;
                Ok(CallbackReturn::Return)
            }),
        );
    }
    let call_state = Rc::clone(&state);
    let metatable = Table::new(&ctx);
    metatable.set_field(
        ctx,
        "__call",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            // stack[0] is the table itself.
            let level = match stack.get(1) {
                LuaValue::String(name) => level_of(name.as_bytes()).ok_or_else(|| {
                    HostError(format!(
                        "log level {:?} is not debug, info, warn or error",
                        name.display_lossy().to_string()
                    ))
                })?,
                _ => {
                    return Err(
                        HostError("morf.log(level, ...) takes a level name first".into()).into(),
                    );
                }
            };
            let message = joined((2..stack.len()).map(|index| text_of(stack.get(index))));
            call_state.borrow_mut().log(level, message);
            stack.clear();
            let _ = ctx;
            Ok(CallbackReturn::Return)
        }),
    );
    log.set_metatable(ctx, Some(metatable));
    morf.set_field(ctx, "log", log);
}
