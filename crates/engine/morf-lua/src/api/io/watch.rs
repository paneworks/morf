//! `morf.fs.watch`: a callback when a file or a directory changes.
//!
//! ```lua
//! local w = morf.fs.watch(path, function(event)
//!     -- event.path, event.kind ("changed" | "created" | "deleted" | "moved"),
//!     -- event.name (relative to a watched directory, else the file's name)
//! end, { recursive = false })
//! w:close()   w:closed()   w:path()
//! ```
//!
//! Every watch sits on the process's one inotify thread (`morf_io::Watch`),
//! which sleeps until the kernel has news and then rings the loop, so an
//! idle watch costs nothing. `poll_services` takes what each watch has
//! gathered, coalesced by path since it last looked, and runs the callback
//! at most [`morf_io::WATCH_BATCH`] times per watch per turn.
//!
//! A watch lives as long as its handle: `:close()` ends it, and so does the
//! handle being collected, a reload, and the runtime going away. A runtime
//! holds at most `Limits::watches` of them (`MORF_LIMITS=watches=N`).

use luna::{
    Callback, CallbackReturn, Context, Executor, Function, Table, UserData, UserRef,
    Value as LuaValue, Variadic,
};
use morf_io::{Watch, WatchHandle, WatchOptions};
use std::cell::RefCell;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::rc::Rc;

use crate::{Limits, reactive_execute::drive_executor, scene_bindings::*, state::*};
use morf_runtime::Handler;

/// The runtime's watches, each owing a Lua callback.
pub(crate) type WatchHub = morf_io::WatchHub<Handler>;
/// A callback owed.
pub(crate) type WatchCall = morf_io::WatchCall<Handler>;

/// A handle as Lua holds it; collected, it closes its watch.
struct WatchToken(WatchHandle);

/// Adds `watch` to `morf.fs`.
pub(crate) fn install_watch_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
    limits: Limits,
) {
    let close = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<WatchToken> = stack.consume(ctx)?;
        token.0.status.close();
        Ok(CallbackReturn::Return)
    });
    let closed = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<WatchToken> = stack.consume(ctx)?;
        stack.replace(ctx, token.0.status.closed());
        Ok(CallbackReturn::Return)
    });
    let path = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<WatchToken> = stack.consume(ctx)?;
        stack.replace(ctx, ctx.intern(token.0.path.as_os_str().as_bytes()));
        Ok(CallbackReturn::Return)
    });
    let methods = Table::new(&ctx);
    methods.set_field(ctx, "close", close);
    methods.set_field(ctx, "closed", closed);
    methods.set_field(ctx, "path", path);
    let metatable = Table::new(&ctx);
    metatable.set_field(ctx, "__index", methods);
    let metatable = ctx.stash(metatable);

    let watch = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (path, callback, options): (LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
        let path = match path {
            LuaValue::String(text) if !text.as_bytes().is_empty() => {
                PathBuf::from(OsStr::from_bytes(text.as_bytes()))
            }
            _ => return Err(HostError("fs.watch needs a path string".into()).into()),
        };
        let LuaValue::Function(Function::Closure(callback)) = callback else {
            return Err(HostError("fs.watch needs a callback function".into()).into());
        };
        let recursive = match options {
            LuaValue::Nil => false,
            LuaValue::Table(options) => match options.get_value(ctx, "recursive") {
                LuaValue::Nil => false,
                LuaValue::Boolean(value) => value,
                _ => return Err(HostError("fs.watch recursive must be boolean".into()).into()),
            },
            _ => return Err(HostError("fs.watch options must be a table".into()).into()),
        };
        let mut state = state.borrow_mut();
        state
            .watches
            .check_room(limits.watches)
            .map_err(HostError)?;
        let watch = match Watch::new(&path, WatchOptions { recursive }) {
            Ok(watch) => watch,
            Err(error) => {
                stack.replace(ctx, (LuaValue::Nil, error.to_string()));
                return Ok(CallbackReturn::Return);
            }
        };
        let callback = crate::vm::handler_store::register(ctx.stash(callback));
        let token = WatchToken(state.watches.add(watch, callback));
        let userdata = UserData::new_static(&ctx, token);
        userdata.set_metatable(ctx, Some(ctx.fetch(&metatable)));
        stack.replace(ctx, userdata);
        Ok(CallbackReturn::Return)
    });
    match morf.get_value(ctx, "fs") {
        LuaValue::Table(fs) => {
            fs.set_field(ctx, "watch", watch);
        }
        _ => {
            let fs = Table::new(&ctx);
            fs.set_field(ctx, "watch", watch);
            morf.set_field(ctx, "fs", fs);
        }
    };
}

/// Runs one owed callback, unless its watch was closed meanwhile (by an
/// earlier callback in the same turn, say).
pub(crate) fn execute_watch_call(
    ctx: Context<'_>,
    call: &WatchCall,
    limits: Limits,
) -> Result<(), String> {
    if !call.live() {
        return Ok(());
    }
    let event = Table::new(&ctx);
    event.set_field(
        ctx,
        "path",
        ctx.intern(call.change.path.as_os_str().as_bytes()),
    );
    event.set_field(ctx, "kind", call.change.kind.as_str());
    event.set_field(
        ctx,
        "name",
        ctx.intern(call.change.name.as_os_str().as_bytes()),
    );
    let executor = Executor::start(
        ctx,
        ctx.fetch(&crate::vm::handler_store::stashed(&call.callback))
            .into(),
        Variadic(vec![LuaValue::Table(event)]),
    );
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}
