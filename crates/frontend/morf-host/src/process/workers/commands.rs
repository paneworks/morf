//! What a worker is asked: shell IPC requests routed to the right output, and
//! the commands each worker carries out on its own runtime.

use morf_io::{IpcReply, IpcRequest, IpcValue as WireValue};
use morf_lua::{Limits, LogEntry, Runtime, Screen};
use std::collections::BTreeMap;
use std::sync::mpsc;
use std::time::Duration;

use crate::{lock::*, paint::*, services::*, supervisor::*, surfaces::*};

pub fn handle_ipc(
    workers: &BTreeMap<String, Worker>,
    daemon_logs: &mut Vec<String>,
    request: &IpcRequest,
) -> IpcReply {
    match request {
        IpcRequest::Call { target, args } => {
            // Every output runs the configuration, so every output hears the
            // verb: which of them acts on it is the configuration's decision
            // (`morf.screens[1]` names the one each instance draws). The
            // reply is the first output's that answered with something, so
            // an instance that stayed quiet does not hide the one that spoke.
            if workers.is_empty() {
                return IpcReply::refused("shell has no active output");
            }
            let args = args.iter().map(lua_ipc_value).collect::<Vec<_>>();
            let mut receivers = Vec::with_capacity(workers.len());
            for worker in workers.values() {
                let (tx, rx) = mpsc::sync_channel(1);
                if worker
                    .commands
                    .send(WorkerCommand::Call {
                        target: target.clone(),
                        args: args.clone(),
                        reply: tx,
                    })
                    .is_ok()
                {
                    receivers.push(rx);
                }
            }
            if receivers.is_empty() {
                return IpcReply::refused("shell output stopped");
            }
            let mut answer: Option<IpcReply> = None;
            let mut refusal: Option<IpcReply> = None;
            for rx in receivers {
                match rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(Ok(values)) => {
                        let reply = IpcReply::success(values.iter().map(wire_ipc_value).collect());
                        if !values.is_empty() {
                            return reply;
                        }
                        answer.get_or_insert(reply);
                    }
                    Ok(Err(error)) => {
                        refusal.get_or_insert(IpcReply::refused(error));
                    }
                    Err(_) => {
                        refusal.get_or_insert(IpcReply::refused("shell output timed out"));
                    }
                }
            }
            answer
                .or(refusal)
                .unwrap_or_else(|| IpcReply::refused("shell output stopped"))
        }
        IpcRequest::Verbs => {
            let mut verbs = Vec::new();
            for worker in workers.values() {
                let (tx, rx) = mpsc::sync_channel(1);
                if worker.commands.send(WorkerCommand::Verbs(tx)).is_ok()
                    && let Ok(found) = rx.recv_timeout(Duration::from_secs(1))
                {
                    verbs.extend(found);
                }
            }
            verbs.sort();
            verbs.dedup();
            IpcReply::success(verbs.into_iter().map(WireValue::String).collect())
        }
        IpcRequest::Log => {
            let mut logs = std::mem::take(daemon_logs);
            for worker in workers.values() {
                let (tx, rx) = mpsc::sync_channel(1);
                if worker.commands.send(WorkerCommand::Logs(tx)).is_ok()
                    && let Ok(found) = rx.recv_timeout(Duration::from_secs(1))
                {
                    logs.extend(found);
                }
            }
            IpcReply::success(logs.into_iter().map(WireValue::String).collect())
        }
        IpcRequest::Capabilities => {
            let mut lines = Vec::new();
            for (output, worker) in workers {
                let (tx, rx) = mpsc::sync_channel(1);
                if worker
                    .commands
                    .send(WorkerCommand::Capabilities(tx))
                    .is_ok()
                    && let Ok(found) = rx.recv_timeout(Duration::from_secs(1))
                {
                    lines.extend(found.into_iter().map(|line| format!("{output}:{line}")));
                }
            }
            IpcReply::success(lines.into_iter().map(WireValue::String).collect())
        }
        IpcRequest::Bindings => {
            let mut bindings = Vec::new();
            for worker in workers.values() {
                let (tx, rx) = mpsc::sync_channel(1);
                if worker.commands.send(WorkerCommand::Bindings(tx)).is_ok()
                    && let Ok(found) = rx.recv_timeout(Duration::from_secs(1))
                {
                    bindings.extend(found);
                }
            }
            bindings.sort();
            bindings.dedup();
            IpcReply::success(bindings.into_iter().map(WireValue::String).collect())
        }
        IpcRequest::Kill => IpcReply::success(Vec::new()),
        // The supervisor answers this before it reaches here; it is the one
        // thing that knows which configuration it is running.
        IpcRequest::Info => IpcReply::refused("info is answered by the supervisor"),
    }
}

#[derive(Clone, Copy, Default)]
pub struct WorkerUpdate {
    pub repaint: bool,
    pub reset_input: bool,
    pub refresh_idle: bool,
    pub recreate_surface: bool,
    /// A new configuration replaced the old one: what the old one set on
    /// the outputs (their gamma) goes with it.
    pub reloaded: bool,
}

pub fn handle_worker_command(
    runtime: &mut Runtime,
    screen: Option<&Screen>,
    policy: LoadPolicy,
    command: WorkerCommand,
) -> WorkerUpdate {
    match command {
        WorkerCommand::Call {
            target,
            args,
            reply,
        } => {
            let result = runtime
                .call_ipc(&target, &args)
                .map_err(|error| error.to_string());
            let repaint = result.is_ok();
            let _ = reply.send(result);
            WorkerUpdate {
                repaint,
                reset_input: false,
                refresh_idle: false,
                recreate_surface: false,
                reloaded: false,
            }
        }
        WorkerCommand::Screens(screens) => {
            let canvas = screen.is_some_and(|screen| screen.name == DESKTOP_CANVAS);
            if canvas {
                runtime.replace_screens(&lua_screens(&screens));
            } else {
                runtime.set_screens(&lua_screens(&screens));
            }
            WorkerUpdate {
                repaint: canvas,
                ..WorkerUpdate::default()
            }
        }
        WorkerCommand::Verbs(reply) => {
            let _ = reply.send(runtime.ipc_verbs());
            WorkerUpdate::default()
        }
        WorkerCommand::Logs(reply) => {
            // Packed here rather than at the socket, because the entry is
            // structured on this side and a string by the time it leaves.
            let _ = reply.send(
                runtime
                    .take_logs()
                    .iter()
                    .map(LogEntry::to_wire)
                    .collect::<Vec<_>>(),
            );
            WorkerUpdate::default()
        }
        WorkerCommand::Capabilities(reply) => {
            let _ = reply.send(runtime.capabilities());
            WorkerUpdate::default()
        }
        WorkerCommand::Bindings(reply) => {
            let _ = reply.send(runtime.binding_dependencies());
            WorkerUpdate::default()
        }
        // Only a worker whose configuration asked to lock is sent this, and
        // it hears it before it has a surface (surface_run.rs).
        WorkerCommand::BecomeLock => WorkerUpdate::default(),
        WorkerCommand::Primary(primary) => WorkerUpdate {
            repaint: runtime.set_primary(primary),
            ..WorkerUpdate::default()
        },
        WorkerCommand::Reload {
            path,
            source,
            hard,
            reply,
        } => {
            // The outputless runtime has no output of its own.
            let mut candidate = match screen {
                Some(screen) => Runtime::for_screen(Limits::from_env().0, screen.clone()),
                None => Runtime::new(Limits::from_env().0),
            };
            // What the compositor and GPU can do did not change with the file,
            // and neither did which runtime is the primary one.
            candidate.set_capabilities(&runtime.capability_pairs());
            candidate.set_primary(runtime.is_primary());
            if !hard {
                candidate.restore_reloadable_state(runtime.reloadable_state());
            }
            // The new file serves the same names the old one did, and it has
            // to find them free: a name refused because the runtime it
            // replaces still held it would be given back a moment later, to
            // nobody.
            runtime.release_bus_names();
            let result = execute_config(&mut candidate, &path, &source, policy)
                // With no output there is nothing to draw, so no root is owed.
                .and_then(|()| match screen {
                    Some(_) => primary_surface_root(&candidate).map(|_| ()),
                    None => Ok(()),
                })
                .and_then(|()| {
                    candidate
                        .update_clock(clock_text())
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                });
            let repaint = match &result {
                Ok(()) => {
                    candidate.dispatch_reload_completed();
                    *runtime = candidate;
                    true
                }
                Err(error) => {
                    runtime.dispatch_reload_failed(error.clone());
                    false
                }
            };
            let _ = reply.send(result);
            WorkerUpdate {
                repaint,
                reset_input: repaint,
                refresh_idle: repaint,
                recreate_surface: repaint && hard,
                reloaded: repaint,
            }
        }
    }
}
