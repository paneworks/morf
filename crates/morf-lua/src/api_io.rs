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
//! each handle at most [`BATCH`] events per turn, so a chatty child shares
//! the loop with everything else. The reactor belongs to the runtime: a
//! reload kills every child the old configuration started (except
//! `detached` ones) and closes its sockets, and nothing it started calls
//! back into the new one.
//!
//! `LD_LIBRARY_PATH` is never inherited: morf may run under a wrapper that
//! points it at store paths a system binary must not load. Pass it in
//! `env` to set it anyway.

use luna::{
    Callback, CallbackReturn, Context, Executor, Function, StashedClosure, StashedTable, Table,
    UserData, UserRef, Value as LuaValue, Variadic,
};
use morf_io::{
    CloseReason, ConnectOptions, Endpoint, IoEvent, IoHandle, IoId, OutputMode, Reactor,
    ReactorControl, SpawnOptions, StdinMode,
};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use crate::{Limits, reactive_execute::drive_executor, scene_bindings::*, state::*, table_menu::*};

/// Children one runtime may have running at once.
const MAX_PROCESSES: usize = 64;
/// Connections one runtime may have open at once.
const MAX_CONNECTIONS: usize = 64;
/// Callbacks one handle may have run per turn of the loop.
pub(crate) const BATCH: usize = 64;
const RUN_DEFAULT_MAX_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_OUTPUT_LIMIT: usize = 256 * 1024 * 1024;
const REQUEST_DEFAULT_MAX: usize = 8 * 1024 * 1024;
const REQUEST_MAX_LIMIT: usize = 64 * 1024 * 1024;
const MAX_WRITE: usize = 1024 * 1024;
const MAX_LINE_LIMIT: usize = 16 * 1024 * 1024;

/// What a handle and its entry share.
#[derive(Default)]
pub(crate) struct HandleStatus {
    running: Cell<bool>,
    connected: Cell<bool>,
    /// Closed by the configuration: no callback runs for it again.
    closed: Cell<bool>,
}

enum Kind {
    Spawn {
        on_stdout: Option<StashedClosure>,
        on_stderr: Option<StashedClosure>,
        on_exit: Option<StashedClosure>,
    },
    Run {
        callback: Option<StashedClosure>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    Connect {
        on_data: Option<StashedClosure>,
        on_connect: Option<StashedClosure>,
        on_close: Option<StashedClosure>,
    },
    Request {
        callback: Option<StashedClosure>,
        reply: Vec<u8>,
        max: usize,
    },
}

struct Entry {
    handle: IoHandle,
    status: Rc<HandleStatus>,
    kind: Kind,
    queue: VecDeque<IoEvent>,
    process: bool,
}

/// A callback owed, with what it is owed.
pub(crate) struct IoCall {
    status: Rc<HandleStatus>,
    callback: StashedClosure,
    args: CallArgs,
}

enum CallArgs {
    None,
    Bytes(Vec<u8>),
    Text(String),
    Exit {
        code: Option<i32>,
        signal: Option<i32>,
        timed_out: bool,
    },
    Run(RunResult),
    Reply(Result<Vec<u8>, String>),
}

#[derive(Default)]
struct RunResult {
    code: Option<i32>,
    signal: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    timed_out: bool,
    truncated: bool,
    error: Option<String>,
}

/// The runtime's processes and connections.
#[derive(Default)]
pub(crate) struct IoHub {
    reactor: Option<Reactor>,
    entries: BTreeMap<IoId, Entry>,
    /// Answers that need no reactor: a `run` whose program would not start.
    deferred: Vec<IoCall>,
}

impl IoHub {
    fn reactor(&mut self) -> Result<&Reactor, HostError> {
        if self.reactor.is_none() {
            self.reactor =
                Some(Reactor::new().map_err(|error| {
                    HostError(format!("cannot start the I/O reactor: {error}"))
                })?);
        }
        Ok(self.reactor.as_ref().expect("just made"))
    }

    fn count(&self, process: bool) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.process == process && !entry.status.closed.get())
            .count()
    }

    /// Takes what the reactor has sent and turns up to [`BATCH`] events per
    /// handle into the callbacks they are owed. The second value says more
    /// is waiting for the next turn.
    pub(crate) fn collect(&mut self) -> (Vec<IoCall>, bool) {
        let mut calls = std::mem::take(&mut self.deferred);
        let Some(reactor) = self.reactor.as_ref() else {
            return (calls, false);
        };
        let control = reactor.control();
        while let Some(event) = reactor.try_next() {
            if let Some(entry) = self.entries.get_mut(&event.id()) {
                entry.queue.push_back(event);
            }
        }
        let mut more = false;
        self.entries.retain(|_, entry| {
            if entry.status.closed.get() {
                return false;
            }
            for _ in 0..BATCH {
                let Some(event) = entry.queue.pop_front() else {
                    return true;
                };
                control.credit(&entry.handle, event.weight());
                let last = event.is_final();
                if !entry.take(event, &mut calls) {
                    if !last {
                        control.close(&entry.handle);
                    }
                    return false;
                }
            }
            more |= !entry.queue.is_empty();
            true
        });
        (calls, more)
    }
}

impl Entry {
    /// Turns one event into what it owes. False when the handle is done.
    fn take(&mut self, event: IoEvent, calls: &mut Vec<IoCall>) -> bool {
        let status = &self.status;
        let mut call = |callback: &Option<StashedClosure>, args| {
            if let Some(callback) = callback {
                calls.push(IoCall {
                    status: Rc::clone(status),
                    callback: callback.clone(),
                    args,
                });
            }
        };
        match (&mut self.kind, event) {
            (Kind::Spawn { on_stdout, .. }, IoEvent::Stdout(_, bytes)) => {
                call(on_stdout, CallArgs::Bytes(bytes));
            }
            (Kind::Spawn { on_stderr, .. }, IoEvent::Stderr(_, bytes)) => {
                call(on_stderr, CallArgs::Bytes(bytes));
            }
            (
                Kind::Spawn { on_exit, .. },
                IoEvent::Exit {
                    code,
                    signal,
                    timed_out,
                    ..
                },
            ) => {
                status.running.set(false);
                call(
                    on_exit,
                    CallArgs::Exit {
                        code,
                        signal,
                        timed_out,
                    },
                );
                return false;
            }
            (Kind::Run { stdout, .. }, IoEvent::Stdout(_, bytes)) => stdout.extend(bytes),
            (Kind::Run { stderr, .. }, IoEvent::Stderr(_, bytes)) => stderr.extend(bytes),
            (
                Kind::Run {
                    callback,
                    stdout,
                    stderr,
                },
                IoEvent::Exit {
                    code,
                    signal,
                    timed_out,
                    truncated,
                    ..
                },
            ) => {
                status.running.set(false);
                let result = RunResult {
                    code,
                    signal,
                    stdout: std::mem::take(stdout),
                    stderr: std::mem::take(stderr),
                    timed_out,
                    truncated,
                    error: None,
                };
                call(callback, CallArgs::Run(result));
                return false;
            }
            (Kind::Connect { on_connect, .. }, IoEvent::Connected(_)) => {
                status.connected.set(true);
                call(on_connect, CallArgs::None);
            }
            (Kind::Connect { on_data, .. }, IoEvent::Data(_, bytes)) => {
                call(on_data, CallArgs::Bytes(bytes));
            }
            (Kind::Connect { on_close, .. }, IoEvent::Closed { reason, .. }) => {
                status.connected.set(false);
                call(on_close, CallArgs::Text(reason.describe()));
                return false;
            }
            (Kind::Request { .. }, IoEvent::Connected(_)) => status.connected.set(true),
            (
                Kind::Request {
                    callback,
                    reply,
                    max,
                },
                IoEvent::Data(_, bytes),
            ) => {
                if reply.len() + bytes.len() > *max {
                    status.connected.set(false);
                    let error = format!("reply exceeds {max} bytes");
                    call(callback, CallArgs::Reply(Err(error)));
                    // Nothing more is wanted from it; `collect` closes it.
                    return false;
                }
                reply.extend(bytes);
            }
            (
                Kind::Request {
                    callback, reply, ..
                },
                IoEvent::Closed { reason, .. },
            ) => {
                status.connected.set(false);
                let answer = match reason {
                    CloseReason::Eof => Ok(std::mem::take(reply)),
                    reason => Err(reason.describe()),
                };
                call(callback, CallArgs::Reply(answer));
                return false;
            }
            _ => {}
        }
        true
    }
}

impl Drop for IoHub {
    fn drop(&mut self) {
        // The reactor goes first, killing what it started, before the
        // callbacks it might have answered are released.
        self.reactor = None;
    }
}

struct IoToken {
    handle: IoHandle,
    control: ReactorControl,
    status: Rc<HandleStatus>,
}

impl IoToken {
    fn close(&self) {
        if !self.status.closed.replace(true) {
            self.control.close(&self.handle);
        }
        self.status.running.set(false);
        self.status.connected.set(false);
    }
}

/// Installs `morf.spawn`, `morf.run`, `morf.connect` and
/// `morf.request_socket`.
pub(crate) fn install_io_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let write = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (token, data): (UserRef<IoToken>, luna::String) = stack.consume(ctx)?;
        if data.as_bytes().len() > MAX_WRITE {
            return Err(HostError(format!("a write is at most {MAX_WRITE} bytes")).into());
        }
        if token.status.closed.get() {
            stack.replace(ctx, (false, "closed"));
            return Ok(CallbackReturn::Return);
        }
        match token.control.write(&token.handle, data.as_bytes().to_vec()) {
            Ok(()) => stack.replace(ctx, true),
            Err(error) => stack.replace(ctx, (false, error.to_string().as_str())),
        }
        Ok(CallbackReturn::Return)
    });
    let close_stdin = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<IoToken> = stack.consume(ctx)?;
        token.control.close_stdin(&token.handle);
        Ok(CallbackReturn::Return)
    });
    let kill = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (token, signal): (UserRef<IoToken>, LuaValue) = stack.consume(ctx)?;
        let signal = match signal {
            LuaValue::Nil => morf_io::signal_number("TERM"),
            LuaValue::Integer(number) => i32::try_from(number)
                .ok()
                .and_then(|number| morf_io::signal_number(&number.to_string())),
            LuaValue::String(name) => morf_io::signal_number(&name.display_lossy().to_string()),
            _ => None,
        }
        .ok_or_else(|| HostError("kill takes a signal name or number".into()))?;
        let running = token.status.running.get();
        if running {
            token.control.signal(&token.handle, signal);
        }
        stack.replace(ctx, running);
        Ok(CallbackReturn::Return)
    });
    let pid = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<IoToken> = stack.consume(ctx)?;
        match token.handle.pid() {
            Some(pid) => stack.replace(ctx, i64::from(pid)),
            None => stack.replace(ctx, LuaValue::Nil),
        }
        Ok(CallbackReturn::Return)
    });
    let running = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<IoToken> = stack.consume(ctx)?;
        stack.replace(ctx, token.status.running.get());
        Ok(CallbackReturn::Return)
    });
    let connected = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<IoToken> = stack.consume(ctx)?;
        stack.replace(ctx, token.status.connected.get());
        Ok(CallbackReturn::Return)
    });
    let close = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let token: UserRef<IoToken> = stack.consume(ctx)?;
        token.close();
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
        let kind = Kind::Run {
            callback: callback.map(|callback| ctx.stash(callback)),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        match run_starter.spawn(ctx, spawn, kind)? {
            Ok(userdata) => stack.replace(ctx, userdata),
            Err(_) => stack.replace(ctx, LuaValue::Nil),
        }
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "run", run);

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
        let mut timeout = Duration::from_secs(5);
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
        let kind = Kind::Request {
            callback,
            reply: Vec::new(),
            max,
        };
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

impl Starter {
    /// Starts a child. The outer error is a refusal (too many running);
    /// the inner one a program that would not start, which `spawn` returns
    /// and `run` answers through its callback.
    fn spawn<'gc>(
        &self,
        ctx: Context<'gc>,
        options: SpawnOptions,
        kind: Kind,
    ) -> Result<Result<UserData<'gc>, String>, HostError> {
        let mut state = self.state.borrow_mut();
        let hub = &mut state.io;
        if hub.count(true) >= MAX_PROCESSES {
            return Err(HostError(format!(
                "more than {MAX_PROCESSES} processes running"
            )));
        }
        let program = options.command[0].clone();
        let handle = match hub.reactor()?.spawn(options) {
            Ok(handle) => handle,
            Err(error) => {
                let message = format!("{program}: {error}");
                if let Kind::Run {
                    callback: Some(callback),
                    ..
                } = kind
                {
                    hub.deferred.push(IoCall {
                        status: Rc::new(HandleStatus::default()),
                        callback,
                        args: CallArgs::Run(RunResult {
                            error: Some(message.clone()),
                            ..RunResult::default()
                        }),
                    });
                    morf_io::wake_all();
                }
                return Ok(Err(message));
            }
        };
        let control = hub.reactor()?.control();
        let status = Rc::new(HandleStatus::default());
        status.running.set(true);
        hub.entries.insert(
            handle.id(),
            Entry {
                handle: handle.clone(),
                status: Rc::clone(&status),
                kind,
                queue: VecDeque::new(),
                process: true,
            },
        );
        let userdata = UserData::new_static(
            &ctx,
            IoToken {
                handle,
                control,
                status,
            },
        );
        userdata.set_metatable(ctx, Some(ctx.fetch(&self.process_metatable)));
        Ok(Ok(userdata))
    }

    fn connect<'gc>(
        &self,
        ctx: Context<'gc>,
        options: ConnectOptions,
        kind: Kind,
    ) -> Result<UserData<'gc>, HostError> {
        let mut state = self.state.borrow_mut();
        let hub = &mut state.io;
        if hub.count(false) >= MAX_CONNECTIONS {
            return Err(HostError(format!(
                "more than {MAX_CONNECTIONS} connections open"
            )));
        }
        let reactor = hub.reactor()?;
        let handle = reactor.connect(options);
        let control = reactor.control();
        let status = Rc::new(HandleStatus::default());
        hub.entries.insert(
            handle.id(),
            Entry {
                handle: handle.clone(),
                status: Rc::clone(&status),
                kind,
                queue: VecDeque::new(),
                process: false,
            },
        );
        let userdata = UserData::new_static(
            &ctx,
            IoToken {
                handle,
                control,
                status,
            },
        );
        userdata.set_metatable(ctx, Some(ctx.fetch(&self.connection_metatable)));
        Ok(userdata)
    }
}

/// What `spawn` and `run` share: the argv and where and how it runs.
fn spawn_options<'gc>(
    ctx: Context<'gc>,
    command: Table<'gc>,
    options: Option<Table<'gc>>,
) -> Result<SpawnOptions, HostError> {
    let command = table_string_array(ctx, command, 256).map_err(HostError)?;
    if command.is_empty() || command[0].is_empty() {
        return Err(HostError("command cannot be empty".into()));
    }
    let mut spawn = SpawnOptions::new(command);
    let Some(options) = options else {
        return Ok(spawn);
    };
    match options.get_value(ctx, "env") {
        LuaValue::Nil => {}
        LuaValue::Table(env) => {
            spawn.environment = table_string_map(ctx, env, 256).map_err(HostError)?;
        }
        _ => return Err(HostError("env must be a table".into())),
    }
    spawn.clear_environment = table_bool(ctx, options, "clear_env", false).map_err(HostError)?;
    match options.get_value(ctx, "cwd") {
        LuaValue::Nil => {}
        LuaValue::String(cwd) => {
            spawn.working_directory = Some(PathBuf::from(cwd.display_lossy().to_string()));
        }
        _ => return Err(HostError("cwd must be a string".into())),
    }
    spawn.stdin = match options.get_value(ctx, "stdin") {
        LuaValue::Nil | LuaValue::Boolean(false) => StdinMode::Null,
        LuaValue::Boolean(true) => StdinMode::Pipe,
        LuaValue::String(text) if text.as_bytes() == b"pipe" => StdinMode::Pipe,
        LuaValue::String(text) => {
            if text.as_bytes().len() > morf_io::MAX_OUTGOING {
                return Err(HostError("stdin is too large".into()));
            }
            StdinMode::Data(text.as_bytes().to_vec())
        }
        _ => return Err(HostError("stdin must be a string, \"pipe\" or nil".into())),
    };
    if let Some(max_line) = positive(ctx, options, "max_line")? {
        spawn.max_line = (max_line as usize).min(MAX_LINE_LIMIT);
    }
    spawn.timeout = positive(ctx, options, "timeout_ms")?.map(Duration::from_millis);
    spawn.detached = table_bool(ctx, options, "detached", false).map_err(HostError)?;
    Ok(spawn)
}

fn endpoint_of<'gc>(ctx: Context<'gc>, options: Table<'gc>) -> Result<Endpoint, HostError> {
    match (
        options.get_value(ctx, "path"),
        options.get_value(ctx, "host"),
        options.get_value(ctx, "port"),
    ) {
        (LuaValue::String(path), LuaValue::Nil, LuaValue::Nil) => Ok(Endpoint::Unix(
            PathBuf::from(path.display_lossy().to_string()),
        )),
        (LuaValue::Nil, LuaValue::String(host), LuaValue::Integer(port))
            if (1..=65535).contains(&port) =>
        {
            Ok(Endpoint::Tcp {
                host: host.display_lossy().to_string(),
                port: port as u16,
            })
        }
        _ => Err(HostError(
            "connect needs path = \"...\" or host = \"...\", port = n".into(),
        )),
    }
}

fn positive<'gc>(
    ctx: Context<'gc>,
    options: Table<'gc>,
    field: &str,
) -> Result<Option<u64>, HostError> {
    match options.get_value(ctx, field) {
        LuaValue::Nil => Ok(None),
        LuaValue::Integer(value) if value > 0 => Ok(Some(value as u64)),
        LuaValue::Number(value) if value.is_finite() && value >= 1.0 => Ok(Some(value as u64)),
        _ => Err(HostError(format!("{field} must be a positive number"))),
    }
}

fn optional_int<'gc>(value: Option<i32>) -> LuaValue<'gc> {
    value.map_or(LuaValue::Nil, |value| LuaValue::Integer(i64::from(value)))
}

/// Runs one owed callback, unless its handle was closed meanwhile.
pub(crate) fn execute_io_call(
    ctx: Context<'_>,
    call: &IoCall,
    limits: Limits,
) -> Result<(), String> {
    if call.status.closed.get() {
        return Ok(());
    }
    let args = match &call.args {
        CallArgs::None => Vec::new(),
        CallArgs::Bytes(bytes) => vec![LuaValue::String(ctx.intern(bytes))],
        CallArgs::Text(text) => vec![LuaValue::String(ctx.intern(text.as_bytes()))],
        CallArgs::Exit {
            code,
            signal,
            timed_out,
        } => vec![
            optional_int(*code),
            optional_int(*signal),
            LuaValue::Boolean(*timed_out),
        ],
        CallArgs::Reply(Ok(reply)) => vec![LuaValue::String(ctx.intern(reply)), LuaValue::Nil],
        CallArgs::Reply(Err(error)) => {
            vec![
                LuaValue::Nil,
                LuaValue::String(ctx.intern(error.as_bytes())),
            ]
        }
        CallArgs::Run(result) => {
            let table = Table::new(&ctx);
            table.set_field(
                ctx,
                "ok",
                result.code == Some(0) && !result.timed_out && result.error.is_none(),
            );
            table.set_field(ctx, "code", optional_int(result.code));
            table.set_field(ctx, "signal", optional_int(result.signal));
            table.set_field(ctx, "stdout", ctx.intern(&result.stdout));
            table.set_field(ctx, "stderr", ctx.intern(&result.stderr));
            table.set_field(ctx, "timed_out", result.timed_out);
            table.set_field(ctx, "truncated", result.truncated);
            if let Some(error) = &result.error {
                table.set_field(ctx, "error", error.as_str());
            }
            vec![LuaValue::Table(table)]
        }
    };
    let executor = Executor::start(ctx, ctx.fetch(&call.callback).into(), Variadic(args));
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}
