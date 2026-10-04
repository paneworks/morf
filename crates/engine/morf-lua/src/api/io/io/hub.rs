//! The hub at work: reactor events queued per handle and turned into owed
//! callbacks, a handle closed, and an owed callback run.

use super::*;

impl IoHub {
    pub(super) fn reactor(&mut self) -> Result<&Reactor, HostError> {
        if self.reactor.is_none() {
            self.reactor =
                Some(Reactor::new().map_err(|error| {
                    HostError(format!("cannot start the I/O reactor: {error}"))
                })?);
        }
        Ok(self.reactor.as_ref().expect("just made"))
    }

    pub(super) fn count(&self, process: bool) -> usize {
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
        let mut call = |callback: &Option<Handler>, args| {
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

impl IoToken {
    pub(super) fn close(&self) {
        if !self.status.closed.replace(true) {
            self.control.close(&self.handle);
        }
        self.status.running.set(false);
        self.status.connected.set(false);
    }
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
    let executor = Executor::start(
        ctx,
        ctx.fetch(&crate::vm::handler_store::stashed(&call.callback))
            .into(),
        Variadic(args),
    );
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

fn optional_int<'gc>(value: Option<i32>) -> LuaValue<'gc> {
    value.map_or(LuaValue::Nil, |value| LuaValue::Integer(i64::from(value)))
}
