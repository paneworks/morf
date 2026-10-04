//! The hub at work: reactor events queued per handle and turned into owed
//! callbacks.

use std::rc::Rc;

use super::{CallArgs, Entry, IO_BATCH, IoCall, IoHub, IoKind, RunResult};
use crate::{CloseReason, IoEvent};

impl<C: Clone> IoHub<C> {
    /// Takes what the reactor has sent and turns up to [`IO_BATCH`] events
    /// per handle into the callbacks they are owed. The second value says
    /// more is waiting for the next turn.
    pub fn collect(&mut self) -> (Vec<IoCall<C>>, bool) {
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
            if entry.status.closed() {
                return false;
            }
            for _ in 0..IO_BATCH {
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

impl<C: Clone> Entry<C> {
    /// Turns one event into what it owes. False when the handle is done.
    fn take(&mut self, event: IoEvent, calls: &mut Vec<IoCall<C>>) -> bool {
        let status = &self.status;
        let mut call = |callback: &Option<C>, args| {
            if let Some(callback) = callback {
                calls.push(IoCall {
                    status: Rc::clone(status),
                    callback: callback.clone(),
                    args,
                });
            }
        };
        match (&mut self.kind, event) {
            (IoKind::Spawn { on_stdout, .. }, IoEvent::Stdout(_, bytes)) => {
                call(on_stdout, CallArgs::Bytes(bytes));
            }
            (IoKind::Spawn { on_stderr, .. }, IoEvent::Stderr(_, bytes)) => {
                call(on_stderr, CallArgs::Bytes(bytes));
            }
            (
                IoKind::Spawn { on_exit, .. },
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
            (IoKind::Run { stdout, .. }, IoEvent::Stdout(_, bytes)) => stdout.extend(bytes),
            (IoKind::Run { stderr, .. }, IoEvent::Stderr(_, bytes)) => stderr.extend(bytes),
            (
                IoKind::Run {
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
            (IoKind::Connect { on_connect, .. }, IoEvent::Connected(_)) => {
                status.connected.set(true);
                call(on_connect, CallArgs::None);
            }
            (IoKind::Connect { on_data, .. }, IoEvent::Data(_, bytes)) => {
                call(on_data, CallArgs::Bytes(bytes));
            }
            (IoKind::Connect { on_close, .. }, IoEvent::Closed { reason, .. }) => {
                status.connected.set(false);
                call(on_close, CallArgs::Text(reason.describe()));
                return false;
            }
            (IoKind::Request { .. }, IoEvent::Connected(_)) => status.connected.set(true),
            (
                IoKind::Request {
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
                IoKind::Request {
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
