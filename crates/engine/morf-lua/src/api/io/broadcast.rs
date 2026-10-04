//! `morf.broadcast(verb, ...)`: one screen's shell calling a verb on every
//! screen's.
//!
//! Each output runs its own copy of the configuration, on its own thread,
//! with its own Lua. Most of the time that is the point -- each draws its
//! own screen -- but some things exist once per session and are wanted on
//! whichever screen the person is at: a polkit agent is registered by one
//! copy, and its dialog belongs where the pointer is. So a copy can say
//! something to all of them, itself included, through the same door a
//! `morf ipc call` comes in by: the shell's own IPC socket, which hands the
//! call to every output's `morf.ipc[verb]`.
//!
//! The calls go out from one thread of their own, in the order they were
//! made, and are not waited for: the supervisor waits on every output's
//! answer, this one's included, and an output blocked on its own broadcast
//! would answer nobody. Arguments are
//! what IPC carries -- nil, booleans, numbers, strings. `false` comes back
//! when there is no shell socket to send through (a headless test, a lock
//! screen), so the caller can do the thing itself.

use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};
use morf_io::IpcValue as WireValue;

use crate::scene_bindings::HostError;

pub use morf_io::set_shell_socket;

fn wire_value(value: LuaValue<'_>) -> Result<WireValue, String> {
    Ok(match value {
        LuaValue::Nil => WireValue::Nil,
        LuaValue::Boolean(value) => WireValue::Boolean(value),
        LuaValue::Integer(value) => WireValue::Integer(value),
        LuaValue::Number(value) => WireValue::Number(value),
        LuaValue::String(text) => WireValue::String(text.display_lossy().to_string()),
        other => return Err(format!("broadcast cannot carry a {}", other.type_name())),
    })
}

pub(crate) fn install_broadcast_api<'gc>(ctx: Context<'gc>, morf: Table<'gc>) {
    let broadcast = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let target = match stack.get(0) {
            LuaValue::String(text) => text.display_lossy().to_string(),
            _ => return Err(HostError("broadcast needs a verb".into()).into()),
        };
        morf_io::check_broadcast_arguments(stack.len().saturating_sub(1)).map_err(HostError)?;
        let mut args = Vec::with_capacity(stack.len().saturating_sub(1));
        for index in 1..stack.len() {
            args.push(wire_value(stack.get(index)).map_err(HostError)?);
        }
        stack.replace(ctx, morf_io::broadcast(target, args));
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "broadcast", broadcast);
}
