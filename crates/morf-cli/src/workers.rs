use morf_io::{IpcReply, IpcRequest, IpcValue as WireValue};
use morf_lua::{Limits, LogEntry, Runtime, Screen};
use morf_wayland::ScreenInfo;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self};
use std::time::Duration;

use crate::{
    config::*, lock::*, outputless::*, paint::*, services::*, supervisor::*, surface_run::*,
    surfaces::*,
};

/// Values a runtime marked `morf.reloadable`, carried to its replacement.
pub(crate) type Seed = BTreeMap<String, morf_lua::IpcValue>;

/// What a worker leaves behind when it ends.
///
/// Its reloadable values, for the runtimes that take over from it across the
/// line between "some outputs" and "none": the outputless runtime starts from
/// what the last output's had, and the outputs that come back start from
/// what it had. And its log lines not yet read, which `morf log` shows with
/// the supervisor's own: an output that went away took them with it before.
#[derive(Clone, Default)]
pub(crate) struct Handover(Arc<Mutex<Left>>);

#[derive(Default)]
struct Left {
    seed: Option<Seed>,
    logs: Vec<String>,
}

impl Handover {
    fn left(&self) -> std::sync::MutexGuard<'_, Left> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Keeps a finished runtime's values and unread log lines; one with no
    /// values leaves what an earlier one kept.
    pub(crate) fn deposit(&self, runtime: &mut Runtime) {
        let seed = runtime.reloadable_state();
        let logs = runtime.take_logs();
        let mut left = self.left();
        if !seed.is_empty() {
            left.seed = Some(seed);
        }
        left.logs.extend(logs.iter().map(LogEntry::to_wire));
    }

    pub(crate) fn take(&self) -> Option<Seed> {
        self.left().seed.take()
    }

    /// The log lines finished workers left, in the order they ended.
    pub(crate) fn take_logs(&self) -> Vec<String> {
        std::mem::take(&mut self.left().logs)
    }
}

/// Everything a worker thread is started with.
pub(crate) struct WorkerStart {
    pub(crate) path: Arc<PathBuf>,
    pub(crate) source: Arc<[u8]>,
    pub(crate) policy: LoadPolicy,
    pub(crate) tx: mpsc::Sender<SupervisorMessage>,
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) commands: mpsc::Receiver<WorkerCommand>,
    /// Reloadable values to start from, across an outputless handover.
    pub(crate) seed: Option<Seed>,
    /// Where this worker leaves its own when it ends.
    pub(crate) handover: Handover,
}

/// The workers the supervisor wants for an output list: one per named
/// output, or -- with none, unless the configuration said it does not want
/// it -- the one outputless runtime, or nothing at all.
pub(crate) fn desired_workers(
    named: BTreeMap<String, ScreenInfo>,
    outputless: Outputless,
) -> BTreeMap<String, ScreenInfo> {
    if !named.is_empty() || outputless == Outputless::Unwanted {
        return named;
    }
    BTreeMap::from([(OUTPUTLESS.to_owned(), outputless_screen())])
}

/// What every worker the supervisor starts shares.
pub(crate) struct WorkerContext<'a> {
    pub(crate) path: &'a Arc<PathBuf>,
    pub(crate) source: &'a Arc<[u8]>,
    pub(crate) policy: LoadPolicy,
    pub(crate) tx: &'a mpsc::Sender<SupervisorMessage>,
    pub(crate) handover: &'a Handover,
}

pub(crate) fn reconcile_workers(
    workers: &mut BTreeMap<String, Worker>,
    desired: &BTreeMap<String, ScreenInfo>,
    context: &WorkerContext<'_>,
) {
    reconcile_with(workers, desired, context.handover, |name, screen, seed| {
        let (commands, command_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let join = spawn_worker(
            name,
            screen,
            WorkerStart {
                path: Arc::clone(context.path),
                source: Arc::clone(context.source),
                policy: context.policy,
                tx: context.tx.clone(),
                stop: Arc::clone(&stop),
                commands: command_rx,
                seed,
                handover: context.handover.clone(),
            },
        );
        Worker {
            stop,
            commands: WorkerSender::new(commands),
            join,
            screen: screen.clone(),
        }
    });
}

/// Brings `workers` to `desired`: stops the ones whose output went or
/// changed, then starts the missing ones with `spawn`. Crossing between the
/// outputless runtime and per-output ones, the runtimes that start are handed
/// the reloadable values the ones that stopped left behind.
pub(crate) fn reconcile_with(
    workers: &mut BTreeMap<String, Worker>,
    desired: &BTreeMap<String, ScreenInfo>,
    handover: &Handover,
    mut spawn: impl FnMut(&str, &ScreenInfo, Option<Seed>) -> Worker,
) {
    let was_outputless = workers.contains_key(OUTPUTLESS);
    let crossing = workers.is_empty() || was_outputless != desired.contains_key(OUTPUTLESS);
    let stale = workers
        .iter()
        .filter(|(name, worker)| desired.get(*name) != Some(&worker.screen))
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    for name in stale {
        let worker = workers.remove(&name).expect("worker key is present");
        worker.request_stop();
        let _ = worker.join.join();
    }
    // Nothing starts: whatever was left is kept for whatever does.
    if desired.keys().all(|name| workers.contains_key(name)) {
        return;
    }
    let seed = handover.take().filter(|_| crossing);
    for (name, screen) in desired {
        if workers.contains_key(name) {
            continue;
        }
        let worker = spawn(name, screen, seed.clone());
        workers.insert(name.clone(), worker);
    }
}

/// Starts the thread that runs the configuration for one output, or for none.
fn spawn_worker(name: &str, screen: &ScreenInfo, start: WorkerStart) -> thread::JoinHandle<()> {
    let output = name.to_owned();
    let screen = screen.clone();
    // Named after the output it drives, so anything reporting per-thread —
    // a profiler, a panic, a frame counter — says which screen it means.
    thread::Builder::new()
        .name(output.clone())
        .spawn(move || {
            let tx = start.tx.clone();
            let stop = Arc::clone(&start.stop);
            let result = if output == OUTPUTLESS {
                run_outputless(start, true)
            } else {
                run_surface(start, screen)
            };
            if let Err(error) = result
                && !stop.load(Ordering::Acquire)
            {
                let _ = tx.send(SupervisorMessage::Worker(WorkerMessage::Failed {
                    output,
                    error,
                }));
            }
        })
        .expect("worker thread")
}

/// Hands every live worker the compositor's new output list, so each runtime's
/// `morf.screens` follows a monitor being plugged in, moved, or unplugged.
pub(crate) fn broadcast_screens(workers: &BTreeMap<String, Worker>, screens: &[ScreenInfo]) {
    for worker in workers.values() {
        let _ = worker
            .commands
            .send(WorkerCommand::Screens(screens.to_vec()));
    }
}

pub(crate) fn handle_ipc(
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
pub(crate) struct WorkerUpdate {
    pub(crate) repaint: bool,
    pub(crate) reset_input: bool,
    pub(crate) refresh_idle: bool,
    pub(crate) recreate_surface: bool,
    /// A new configuration replaced the old one: what the old one set on
    /// the outputs (their gamma) goes with it.
    pub(crate) reloaded: bool,
}

pub(crate) fn handle_worker_command(
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
            runtime.set_screens(&lua_screens(&screens));
            WorkerUpdate::default()
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
            // What the compositor and GPU can do did not change with the file.
            candidate.set_capabilities(&runtime.capability_pairs());
            if !hard {
                candidate.restore_reloadable_state(runtime.reloadable_state());
            }
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
