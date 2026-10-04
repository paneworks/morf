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
//! at most [`BATCH`] times per watch per turn.
//!
//! A watch lives as long as its handle: `:close()` ends it, and so does the
//! handle being collected, a reload, and the runtime going away. A runtime
//! holds at most `Limits::watches` of them (`MORF_LIMITS=watches=N`).

use luna::{
    Callback, CallbackReturn, Context, Executor, Function, Table, UserData, UserRef,
    Value as LuaValue, Variadic,
};
use morf_io::{FsChange, Watch, WatchOptions};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::rc::Rc;

use crate::{Limits, reactive_execute::drive_executor, scene_bindings::*, state::*};
use morf_runtime::Handler;

/// Callbacks one watch may have run per turn of the loop.
const BATCH: usize = 64;

/// What a handle and its entry share.
#[derive(Default)]
pub(crate) struct WatchStatus {
    closed: Cell<bool>,
}

struct Entry {
    watch: Watch,
    callback: Handler,
    status: Rc<WatchStatus>,
    queue: VecDeque<FsChange>,
}

/// A callback owed.
pub(crate) struct WatchCall {
    status: Rc<WatchStatus>,
    callback: Handler,
    change: FsChange,
}

/// The runtime's watches.
#[derive(Default)]
pub(crate) struct WatchHub {
    entries: Vec<Entry>,
}

impl WatchHub {
    /// Open watches.
    pub(crate) fn len(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| !entry.status.closed.get())
            .count()
    }

    /// What each watch gathered, as the callbacks it is owed; up to
    /// [`BATCH`] per watch. The second value says more is waiting.
    pub(crate) fn collect(&mut self) -> (Vec<WatchCall>, bool) {
        let mut calls = Vec::new();
        let mut more = false;
        self.entries.retain_mut(|entry| {
            if entry.status.closed.get() {
                return false;
            }
            entry.queue.extend(entry.watch.drain());
            for _ in 0..BATCH {
                let Some(change) = entry.queue.pop_front() else {
                    break;
                };
                calls.push(WatchCall {
                    status: Rc::clone(&entry.status),
                    callback: entry.callback.clone(),
                    change,
                });
            }
            more |= !entry.queue.is_empty();
            true
        });
        (calls, more)
    }
}

struct WatchToken {
    status: Rc<WatchStatus>,
    path: PathBuf,
}

impl Drop for WatchToken {
    /// Collected: nobody can close it any more, so it closes itself. The
    /// hub lets go of the kernel's watch on its next turn.
    fn drop(&mut self) {
        self.status.closed.set(true);
    }
}

/// Adds `watch` to `morf.fs`.
pub(crate) fn install_watch_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
    limits: Limits,
) {
    let close = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<WatchToken> = stack.consume(ctx)?;
        token.status.closed.set(true);
        Ok(CallbackReturn::Return)
    });
    let closed = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<WatchToken> = stack.consume(ctx)?;
        stack.replace(ctx, token.status.closed.get());
        Ok(CallbackReturn::Return)
    });
    let path = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<WatchToken> = stack.consume(ctx)?;
        stack.replace(ctx, ctx.intern(token.path.as_os_str().as_bytes()));
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
        if state.watches.len() >= limits.watches {
            return Err(HostError(format!(
                "more than {} watches open (MORF_LIMITS=watches=N)",
                limits.watches
            ))
            .into());
        }
        let watch = match Watch::new(&path, WatchOptions { recursive }) {
            Ok(watch) => watch,
            Err(error) => {
                stack.replace(ctx, (LuaValue::Nil, error.to_string()));
                return Ok(CallbackReturn::Return);
            }
        };
        let status = Rc::new(WatchStatus::default());
        let token = WatchToken {
            status: Rc::clone(&status),
            path: watch.path().to_path_buf(),
        };
        state.watches.entries.push(Entry {
            watch,
            callback: crate::vm::handler_store::register(ctx.stash(callback)),
            status,
            queue: VecDeque::new(),
        });
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
    if call.status.closed.get() {
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
