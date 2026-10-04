//! `morf.spawn`, `morf.run`, `morf.connect`, `morf.request_socket`:
//! processes and sockets whose output arrives as callbacks.
//!
//! ```lua
//! local child = morf.spawn {
//!     command = { "pactl", "subscribe" },   -- argv; never a shell
//!     env = { LANG = "C" },                  -- added to what is inherited
//!     clear_env = false,                     -- start from nothing instead
//!     cwd = "/tmp",
//!     stdin = "text" | "pipe" | nil,          -- bytes then EOF, kept open, or /dev/null
//!     lines = true,                          -- false: raw chunks
//!     max_line = 65536,                      -- longer lines are cut there
//!     on_stdout = function(line) end,        -- absent: stdout is /dev/null
//!     on_stderr = function(line) end,        -- absent: morf's own stderr
//!     on_exit = function(code, signal, timed_out) end,
//!     timeout_ms = 5000,                     -- TERM, then KILL two seconds later
//!     max_output = 1024 * 1024,              -- past this, output is dropped
//!     detached = false,                      -- true: outlives a reload
//! }
//! child:write("data")  child:close_stdin()  child:kill("TERM")
//! child:pid()  child:running()  child:close()  -- close: no more callbacks
//!
//! morf.run({ "git", "status", "--short" }, { cwd = dir }, function(result)
//!     -- result: ok, code, signal, stdout, stderr, timed_out, truncated, error
//! end)
//!
//! local conn = morf.connect {
//!     path = "/run/user/1000/some.sock",      -- or host = "127.0.0.1", port = 7000
//!     on_line = function(line) end,          -- or on_data = function(chunk) end
//!     on_connect = function() end,
//!     on_close = function(reason) end,       -- "eof", "timed out", or the error
//!     connect_timeout_ms = 5000,
//! }
//! conn:send("hello\n")  conn:connected()  conn:close()
//!
//! morf.request_socket(path, "j/monitors", function(reply, err) end,
//!     { timeout_ms = 5000, max_bytes = 8 * 1024 * 1024 })
//! ```
//!
//! Everything is started on the spot, from anywhere: the top level, a
//! handler, a timer, another callback. A reactor thread (`morf_io::Reactor`)
//! watches the pipes and sockets and wakes the loop; `poll_services` hands
//! each handle at most [`morf_io::IO_BATCH`] events per turn, so a chatty
//! child shares the loop with everything else. The reactor belongs to the runtime: a
//! reload kills every child the old configuration started (except
//! `detached` ones) and closes its sockets, and nothing it started calls
//! back into the new one.
//!
//! `LD_LIBRARY_PATH` is never inherited: morf may run under a wrapper that
//! points it at store paths a system binary must not load. Pass it in
//! `env` to set it anyway.

use luna::{
    Callback, CallbackReturn, Context, Executor, Function, StashedTable, Table, UserData, UserRef,
    Value as LuaValue, Variadic,
};
use morf_io::{
    CallArgs, ConnectOptions, Endpoint, IoLink, MAX_LINE_LIMIT, MAX_OUTPUT_LIMIT, OutputMode,
    REQUEST_DEFAULT_MAX, REQUEST_DEFAULT_TIMEOUT, REQUEST_MAX_LIMIT, RUN_DEFAULT_MAX_OUTPUT,
    SpawnOptions, StdinMode,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use crate::{Limits, reactive_execute::drive_executor, scene_bindings::*, state::*, table_menu::*};
use morf_runtime::Handler;

mod hub;
mod start;

pub(crate) use hub::execute_io_call;
use start::{endpoint_of, positive, signal_of, spawn_options};

/// The runtime's processes and connections, each owing Lua callbacks.
pub(crate) type IoHub = morf_io::IoHub<Handler>;
/// A callback owed, with what it is owed.
pub(crate) type IoCall = morf_io::IoCall<Handler>;
type Kind = morf_io::IoKind<Handler>;

/// A child or a connection as Lua holds it.
struct IoToken(IoLink);

/// Installs `morf.spawn`, `morf.run`, `morf.connect` and
/// `morf.request_socket`.
pub(crate) fn install_io_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let write = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (token, data): (UserRef<IoToken>, luna::String) = stack.consume(ctx)?;
        match token.0.write(data.as_bytes()).map_err(HostError)? {
            Ok(()) => stack.replace(ctx, true),
            Err(error) => stack.replace(ctx, (false, error.as_str())),
        }
        Ok(CallbackReturn::Return)
    });
    let close_stdin = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<IoToken> = stack.consume(ctx)?;
        token.0.close_stdin();
        Ok(CallbackReturn::Return)
    });
    let kill = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (token, signal): (UserRef<IoToken>, LuaValue) = stack.consume(ctx)?;
        let signal = signal_of(signal)?;
        stack.replace(ctx, token.0.kill(signal));
        Ok(CallbackReturn::Return)
    });
    let pid = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<IoToken> = stack.consume(ctx)?;
        match token.0.pid() {
            Some(pid) => stack.replace(ctx, i64::from(pid)),
            None => stack.replace(ctx, LuaValue::Nil),
        }
        Ok(CallbackReturn::Return)
    });
    let running = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<IoToken> = stack.consume(ctx)?;
        stack.replace(ctx, token.0.status().running());
        Ok(CallbackReturn::Return)
    });
    let connected = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<IoToken> = stack.consume(ctx)?;
        stack.replace(ctx, token.0.status().connected());
        Ok(CallbackReturn::Return)
    });
    let close = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<IoToken> = stack.consume(ctx)?;
        token.0.close();
        Ok(CallbackReturn::Return)
    });

    let process_methods = Table::new(&ctx);
    process_methods.set_field(ctx, "write", write);
    process_methods.set_field(ctx, "close_stdin", close_stdin);
    process_methods.set_field(ctx, "kill", kill);
    process_methods.set_field(ctx, "pid", pid);
    process_methods.set_field(ctx, "running", running);
    process_methods.set_field(ctx, "close", close);
    let process_metatable = Table::new(&ctx);
    process_metatable.set_field(ctx, "__index", process_methods);
    let connection_methods = Table::new(&ctx);
    connection_methods.set_field(ctx, "send", write);
    connection_methods.set_field(ctx, "connected", connected);
    connection_methods.set_field(ctx, "close", close);
    let connection_metatable = Table::new(&ctx);
    connection_metatable.set_field(ctx, "__index", connection_methods);

    let starter = Rc::new(Starter {
        state,
        process_metatable: ctx.stash(process_metatable),
        connection_metatable: ctx.stash(connection_metatable),
    });

    let spawn_starter = Rc::clone(&starter);
    let spawn = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let options: Table = stack.consume(ctx)?;
        let command = match options.get_value(ctx, "command") {
            LuaValue::Table(command) => command,
            _ => return Err(HostError("spawn needs command = { argv... }".into()).into()),
        };
        let mut spawn = spawn_options(ctx, command, Some(options))?;
        let on_stdout = optional_closure(ctx, options, "on_stdout").map_err(HostError)?;
        let on_stderr = optional_closure(ctx, options, "on_stderr").map_err(HostError)?;
        let on_exit = optional_closure(ctx, options, "on_exit").map_err(HostError)?;
        if on_stdout.is_none() {
            spawn.stdout = OutputMode::Null;
        }
        if on_stderr.is_none() {
            spawn.stderr = OutputMode::Inherit;
        }
        spawn.lines = table_bool(ctx, options, "lines", true).map_err(HostError)?;
        spawn.max_output = positive(ctx, options, "max_output")?
            .map(|bytes| (bytes as usize).min(MAX_OUTPUT_LIMIT));
        let kind = Kind::Spawn {
            on_stdout,
            on_stderr,
            on_exit,
        };
        match spawn_starter.spawn(ctx, spawn, kind)? {
            Ok(userdata) => stack.replace(ctx, userdata),
            Err(error) => stack.replace(ctx, (LuaValue::Nil, error.as_str())),
        }
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "spawn", spawn);

    let run_starter = Rc::clone(&starter);
    let run = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (command, options, callback): (Table, LuaValue, LuaValue) = stack.consume(ctx)?;
        let (options, callback) = match (options, callback) {
            (LuaValue::Function(Function::Closure(callback)), LuaValue::Nil) => {
                (None, Some(callback))
            }
            (LuaValue::Nil, LuaValue::Nil) => (None, None),
            (LuaValue::Table(options), LuaValue::Function(Function::Closure(callback))) => {
                (Some(options), Some(callback))
            }
            (LuaValue::Nil, LuaValue::Function(Function::Closure(callback))) => {
                (None, Some(callback))
            }
            (LuaValue::Table(options), LuaValue::Nil) => (Some(options), None),
            _ => {
                return Err(HostError("run takes (argv, [options], callback)".into()).into());
            }
        };
        let mut spawn = spawn_options(ctx, command, options)?;
        spawn.lines = false;
        spawn.max_output = Some(match options {
            Some(options) => positive(ctx, options, "max_output")?
                .map_or(RUN_DEFAULT_MAX_OUTPUT, |bytes| {
                    (bytes as usize).min(MAX_OUTPUT_LIMIT)
                }),
            None => RUN_DEFAULT_MAX_OUTPUT,
        });
        let kind = Kind::run(
            callback.map(|callback| crate::vm::handler_store::register(ctx.stash(callback))),
        );
        match run_starter.spawn(ctx, spawn, kind)? {
            Ok(userdata) => stack.replace(ctx, userdata),
            Err(_) => stack.replace(ctx, LuaValue::Nil),
        }
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "run", run);
    // `morf.kill(pid, signal)`: a signal to a process morf did not start
    // (or no longer holds the handle of) -- a recorder left running by an
    // earlier shell. The default is TERM. Process groups (pid <= 0) and init
    // are refused. Returns true, or false and the reason.
    let kill_pid = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (pid, signal): (i64, LuaValue) = stack.consume(ctx)?;
        let pid = morf_io::signalable_pid(pid).map_err(HostError)?;
        let signal = signal_of(signal)?;
        match morf_io::signal_process(pid, signal) {
            Ok(()) => stack.replace(ctx, true),
            Err(error) => stack.replace(ctx, (false, error.to_string().as_str())),
        }
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "kill", kill_pid);

    let connect_starter = Rc::clone(&starter);
    let connect = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let options: Table = stack.consume(ctx)?;
        let endpoint = endpoint_of(ctx, options)?;
        let on_line = optional_closure(ctx, options, "on_line").map_err(HostError)?;
        let on_data = optional_closure(ctx, options, "on_data").map_err(HostError)?;
        if on_line.is_some() && on_data.is_some() {
            return Err(HostError("connect takes on_line or on_data, not both".into()).into());
        }
        let mut connect = ConnectOptions::new(endpoint);
        connect.lines = on_line.is_some();
        if let Some(max_line) = positive(ctx, options, "max_line")? {
            connect.max_line = (max_line as usize).min(MAX_LINE_LIMIT);
        }
        if let Some(timeout) = positive(ctx, options, "connect_timeout_ms")? {
            connect.connect_timeout = Duration::from_millis(timeout);
        }
        let kind = Kind::Connect {
            on_data: on_line.or(on_data),
            on_connect: optional_closure(ctx, options, "on_connect").map_err(HostError)?,
            on_close: optional_closure(ctx, options, "on_close").map_err(HostError)?,
        };
        stack.replace(ctx, connect_starter.connect(ctx, connect, kind)?);
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "connect", connect);

    let request = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (path, data, callback, options): (luna::String, luna::String, LuaValue, LuaValue) =
            stack.consume(ctx)?;
        let callback = match callback {
            LuaValue::Nil => None,
            LuaValue::Function(Function::Closure(callback)) => Some(ctx.stash(callback)),
            _ => return Err(HostError("request_socket callback must be a function".into()).into()),
        };
        let options = match options {
            LuaValue::Nil => None,
            LuaValue::Table(options) => Some(options),
            _ => return Err(HostError("request_socket options must be a table".into()).into()),
        };
        let mut connect = ConnectOptions::new(Endpoint::Unix(PathBuf::from(
            path.display_lossy().to_string(),
        )));
        let mut timeout = REQUEST_DEFAULT_TIMEOUT;
        let mut max = REQUEST_DEFAULT_MAX;
        if let Some(options) = options {
            if let Some(ms) = positive(ctx, options, "timeout_ms")? {
                timeout = Duration::from_millis(ms);
            }
            if let Some(bytes) = positive(ctx, options, "max_bytes")? {
                max = (bytes as usize).min(REQUEST_MAX_LIMIT);
            }
        }
        connect.connect_timeout = timeout;
        connect.deadline = Some(timeout);
        connect.greeting = data.as_bytes().to_vec();
        let kind = Kind::request(callback.map(crate::vm::handler_store::register), max);
        stack.replace(ctx, starter.connect(ctx, connect, kind)?);
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "request_socket", request);
}

struct Starter {
    state: Rc<RefCell<ReactiveState>>,
    process_metatable: StashedTable,
    connection_metatable: StashedTable,
}
