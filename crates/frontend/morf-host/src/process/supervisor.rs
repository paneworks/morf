mod config;
mod failure;
mod follow;
mod outputs;

use morf_app::{LayerClient, Output};
use morf_io::{IpcReply, IpcRequest, IpcServer, IpcValue as WireValue};
use morf_lua::{LogEntry, LogLevel};
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self};
use std::time::{Duration, Instant};

use crate::process::socket_path::socket_path;
use crate::{lock::*, paint::outputless::*, process::services::*, process::workers::*};

pub use config::{
    collect_lua_scripts, execute_config, execute_config_on, execute_config_with, runtime_scripts,
};
pub use failure::{FailureStep, PANICKED, SURFACE_CLOSED, failure_step, is_panic};
pub use follow::{RELOAD_SETTLE, follow_lua_files, lua_snapshot};
pub use outputs::{OUTPUTS, known_outputs, lua_screen, lua_screens, named_screens, store_outputs};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadPolicy {
    pub plugins: bool,
    pub external_roots: bool,
}

impl Default for LoadPolicy {
    fn default() -> Self {
        Self {
            plugins: true,
            external_roots: true,
        }
    }
}

/// Cage presents one fullscreen view across its output layout. A greeter
/// there needs one controller/canvas, rather than overlapping output workers.
pub const DESKTOP_CANVAS: &str = "@desktop-canvas";

/// Sends `Probe` after `delay`, from a thread of its own.
fn probe_later(tx: &mpsc::Sender<SupervisorMessage>, delay: Duration) {
    let tx = tx.clone();
    thread::spawn(move || {
        thread::sleep(delay);
        let _ = tx.send(SupervisorMessage::Probe);
    });
}

/// Packs one of the supervisor's own messages the way a worker's arrive.
///
/// The supervisor has no runtime to log through, and its lines join the
/// workers' in one list -- so they are packed here rather than arriving bare
/// and being shown without a level beside lines that have one.
fn daemon_log(message: String) -> String {
    LogEntry {
        level: LogLevel::Warn,
        at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_millis() as u64)
            .unwrap_or(0),
        message,
    }
    .to_wire()
}

pub fn supervise(path: PathBuf, source: Vec<u8>, policy: LoadPolicy) -> Result<(), String> {
    let probe = LayerClient::probe().map_err(|error| error.to_string())?;
    let desktop_canvas = !probe.supports_layer_shell()
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "greet");
    let mut named = named_screens(probe.screens())?;
    // Seeded before the first worker exists, so the very first configuration
    // load already sees every output and not only the one it draws to.
    store_outputs(probe.screens());
    drop(probe);
    // With no output at all, the configuration is run once with none to
    // hear whether it wants to be (`morf.surface.outputless`); one that does
    // not is stopped at once and the shell waits for an output.
    let mut outputless = Outputless::Unknown;
    let handover = Handover::default();
    let mut probing = false;
    // The file says what it is, and the only way to hear it is to run it.
    // The workers run it, once each, and say what it asked to be: one that
    // asks to be a session lock is one client for every output, not the
    // shape the workers have, so the first worker to say so becomes that
    // client with the runtime it already has and the rest stop. The file
    // used to be run once more here first, and thrown away: whatever that
    // run did at once and meant to finish later -- a file written now and
    // another on a timer, a program started, a bus name taken -- was done
    // twice, or left half done.
    let path = Arc::new(path);
    let mut source: Arc<[u8]> = source.into();
    let (tx, rx) = mpsc::channel();
    let reload_roots = runtimepath_roots(&path, policy.external_roots);
    let watch_files = Arc::new(AtomicBool::new(true));
    let watcher_enabled = Arc::clone(&watch_files);
    let reload_tx = tx.clone();
    thread::spawn(move || {
        follow_lua_files(&reload_roots, &watcher_enabled, |_| {
            reload_tx
                .send(SupervisorMessage::Reload {
                    hard: false,
                    output: None,
                })
                .is_ok()
        });
    });
    let started = std::time::Instant::now();
    // Taken before any worker runs the file, so a second shell on this
    // display stops before it does anything. A lock runs beside the shell
    // and answers on a socket of its own, so a taken socket is only an
    // error once the file has shown it is not a lock.
    let (mut server, owner, taken) = match bind_shell_socket(&tx) {
        Ok((server, owner)) => {
            // `morf.broadcast` sends through it: one output's shell calling
            // every output's.
            if let Ok(path) = socket_path() {
                morf_lua::set_shell_socket(path);
            }
            (Some(server), owner, None)
        }
        Err(error) => (None, 0, Some(error)),
    };
    let mut workers = BTreeMap::new();
    // The runtime that does what must be done once (`morf.primary()`).
    let mut primary: Option<String> = None;
    let mut daemon_logs = Vec::new();
    let mut closures = VecDeque::new();
    let mut crashes = VecDeque::new();
    // Brings the workers to the output list and what the configuration said
    // it wants with none; with no worker at all, the compositor is asked
    // again until an output comes.
    macro_rules! settle {
        () => {{
            if named.is_empty() {
                // Surfaces closed because their outputs went are explained.
                closures.clear();
            }
            reconcile_workers(
                &mut workers,
                &if desktop_canvas {
                    BTreeMap::from([(
                        DESKTOP_CANVAS.to_owned(),
                        Output {
                            name: Some(DESKTOP_CANVAS.to_owned()),
                            ..Output::default()
                        },
                    )])
                } else {
                    desired_workers(named.clone(), outputless)
                },
                &mut primary,
                &WorkerContext {
                    path: &path,
                    source: &source,
                    policy,
                    tx: &tx,
                    handover: &handover,
                },
            );
            if workers.is_empty() && !probing {
                probing = true;
                probe_later(&tx, Duration::from_secs(1));
            }
        }};
    }
    settle!();

    loop {
        match rx.recv() {
            Ok(SupervisorMessage::Worker(WorkerMessage::Screens { output, screens }))
                if workers.contains_key(&output) =>
            {
                // One worker's Wayland client noticed the change; every other
                // worker has to hear about it too, including the ones this
                // reconcile leaves running, or their `morf.screens` keeps
                // describing a monitor that has gone away.
                if store_outputs(&screens) {
                    broadcast_screens(&workers, &screens);
                }
                named = named_screens(&screens)?;
                settle!();
            }
            Ok(SupervisorMessage::Worker(WorkerMessage::Screens { .. })) => {}
            Ok(SupervisorMessage::Worker(WorkerMessage::Outputless { output, wanted }))
                if workers.contains_key(&output) =>
            {
                outputless = Outputless::from_flag(wanted);
                if named.is_empty() {
                    settle!();
                }
            }
            Ok(SupervisorMessage::Worker(WorkerMessage::Outputless { .. })) => {}
            Ok(SupervisorMessage::Worker(WorkerMessage::Loaded {
                output,
                session_lock,
                outputless: wanted,
            })) => match loaded_step(session_lock, taken.is_some()) {
                LoadedStep::Run => {
                    outputless = Outputless::from_flag(wanted);
                    if output == OUTPUTLESS && !wanted {
                        daemon_logs.push(daemon_log(
                            "the compositor offers no output, and the configuration does not \
                             run without one (morf.surface.outputless); waiting for an output"
                                .to_owned(),
                        ));
                        settle!();
                    }
                }
                LoadedStep::Refuse => {
                    stop_workers(workers);
                    return Err(taken.unwrap_or_default());
                }
                LoadedStep::Lock => {
                    drop(server.take());
                    let Some(chosen) = workers.remove(&output) else {
                        continue;
                    };
                    stop_workers(std::mem::take(&mut workers));
                    if chosen.commands.send(WorkerCommand::BecomeLock).is_err() {
                        return Err(format!("output {output}: stopped before it could lock"));
                    }
                    let _ = chosen.join.join();
                    // A lock that failed says so the way a worker does.
                    while let Ok(message) = rx.try_recv() {
                        if let SupervisorMessage::Worker(WorkerMessage::Failed { output, error }) =
                            message
                        {
                            return Err(format!("output {output}: {error}"));
                        }
                    }
                    return Ok(());
                }
            },
            Ok(SupervisorMessage::Worker(WorkerMessage::Failed { output, error })) => {
                let window = if is_panic(&error) {
                    &mut crashes
                } else {
                    &mut closures
                };
                let step = failure_step(&error, window, Instant::now());
                if step == FailureStep::Stop {
                    stop_workers(workers);
                    return Err(format!("output {output}: {error}"));
                }
                if step == FailureStep::Restart {
                    // Only that output's runtime ended; it starts over with
                    // the primary duty it held, the others never stop.
                    if let Some(worker) = workers.remove(&output) {
                        let _ = worker.join.join();
                    }
                    daemon_logs.push(daemon_log(format!(
                        "output {output}: {error}; restarting it"
                    )));
                    settle!();
                    continue;
                }
                // That output is going away; the others are not. Asked again
                // shortly, once the compositor has said what is left.
                if let Some(worker) = workers.remove(&output) {
                    worker.request_stop();
                    let _ = worker.join.join();
                }
                // The primary runtime's output went: the duty moves now, to
                // an output still lit, rather than after the next probe --
                // the runtime that held it has ended, its bus names with it.
                if primary.as_deref() == Some(output.as_str()) {
                    hand_over_primary(&workers, &mut primary);
                }
                daemon_logs.push(daemon_log(format!(
                    "output {output}: {error}; asking the compositor for its outputs again"
                )));
                if !probing {
                    probing = true;
                    probe_later(&tx, Duration::from_millis(250));
                }
            }
            Ok(SupervisorMessage::Probe) => {
                probing = false;
                match LayerClient::probe() {
                    Ok(probe) => {
                        let screens = probe.screens().to_vec();
                        drop(probe);
                        if store_outputs(&screens) {
                            broadcast_screens(&workers, &screens);
                        }
                        named = named_screens(&screens)?;
                        settle!();
                    }
                    Err(error) => {
                        daemon_logs.push(daemon_log(format!("asking for the outputs: {error}")));
                        // With no worker there is no connection to hear an
                        // output come back on, so the compositor is asked
                        // again, only while there is none.
                        if workers.is_empty() {
                            probing = true;
                            probe_later(&tx, Duration::from_secs(1));
                        }
                    }
                }
            }
            Ok(SupervisorMessage::Ipc(incoming)) => {
                if incoming.peer.uid != owner {
                    incoming.reply(IpcReply::refused("peer uid does not own the shell"));
                    continue;
                }
                let kill = matches!(incoming.request, IpcRequest::Kill);
                // Answered here rather than in `handle_ipc`, because the
                // supervisor is the one thing that knows which configuration
                // it is running and when it began.
                let reply = if matches!(incoming.request, IpcRequest::Info) {
                    IpcReply::success(vec![
                        WireValue::Integer(i64::from(std::process::id())),
                        WireValue::String(path.to_string_lossy().into_owned()),
                        WireValue::Integer(started.elapsed().as_secs() as i64),
                    ])
                } else {
                    if matches!(incoming.request, IpcRequest::Log) {
                        daemon_logs.extend(handover.take_logs());
                    }
                    handle_ipc(&workers, &mut daemon_logs, &incoming.request)
                };
                incoming.reply(reply);
                if kill {
                    stop_workers(workers);
                    drop(server);
                    return Ok(());
                }
            }
            Ok(SupervisorMessage::Reload { hard, output: only }) => {
                match fs::read(path.as_ref()) {
                    Ok(bytes) => {
                        source = Arc::from(bytes);
                        // Every output, or the one that asked alone.
                        let asked = |name: &String| only.as_ref().is_none_or(|only| only == name);
                        for (output, worker) in workers.iter().filter(|(name, _)| asked(name)) {
                            let (reply, result) = mpsc::sync_channel(1);
                            if worker
                                .commands
                                .send(WorkerCommand::Reload {
                                    path: Arc::clone(&path),
                                    source: Arc::clone(&source),
                                    hard,
                                    reply,
                                })
                                .is_err()
                            {
                                daemon_logs
                                    .push(daemon_log(format!("reload {output}: output stopped")));
                                continue;
                            }
                            match result.recv_timeout(Duration::from_secs(2)) {
                                Ok(Ok(())) => {}
                                Ok(Err(error)) => daemon_logs
                                    .push(daemon_log(format!("reload {output}: {error}"))),
                                Err(_) => daemon_logs
                                    .push(daemon_log(format!("reload {output}: timed out"))),
                            }
                        }
                    }
                    Err(error) => daemon_logs.push(daemon_log(format!("reload: {error}"))),
                }
            }
            Ok(SupervisorMessage::Quit) => {
                // The same shutdown `morf kill` performs, asked for from the
                // inside. A greeter that has launched its session has nothing
                // left to draw, and until now had no way to say so.
                stop_workers(workers);
                drop(server);
                return Ok(());
            }
            Ok(SupervisorMessage::WatchFiles(enabled)) => {
                watch_files.store(enabled, Ordering::Release);
            }
            Err(_) => return Err("all output workers stopped".to_owned()),
        }
    }
}

/// What the supervisor does when a worker has run the configuration.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadedStep {
    /// A shell: keep going.
    Run,
    /// A shell on a display whose socket another one holds: stop.
    Refuse,
    /// A session lock: that worker becomes the lock, the others stop.
    Lock,
}

pub fn loaded_step(session_lock: bool, socket_taken: bool) -> LoadedStep {
    if session_lock {
        LoadedStep::Lock
    } else if socket_taken {
        LoadedStep::Refuse
    } else {
        LoadedStep::Run
    }
}

/// The shell's IPC socket, with its requests forwarded to `tx`, and the uid
/// that owns it.
fn bind_shell_socket(tx: &mpsc::Sender<SupervisorMessage>) -> Result<(IpcServer, u32), String> {
    let (ipc_tx, ipc_rx) = mpsc::channel();
    let socket = socket_path()?;
    let server = IpcServer::bind(&socket, ipc_tx).map_err(|error| {
        // A socket left behind by a killed process is reclaimed by `bind`
        // itself, so the only way this address is in use is another live
        // instance. Saying which display it is on is what makes the message
        // actionable: one morf owns one `WAYLAND_DISPLAY`, and the fix is to
        // stop that one rather than to go looking for a stale file.
        if error.kind() == std::io::ErrorKind::AddrInUse {
            let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "?".to_owned());
            return format!(
                "another morf is already running on display {display}; \
                 stop it before starting another (socket {})",
                socket.display()
            );
        }
        format!("could not bind IPC socket {}: {error}", socket.display())
    })?;
    let owner = fs::metadata(&socket)
        .map_err(|error| format!("could not inspect IPC socket: {error}"))?
        .uid();
    let forward = tx.clone();
    thread::spawn(move || {
        while let Ok(request) = ipc_rx.recv() {
            if forward.send(SupervisorMessage::Ipc(request)).is_err() {
                break;
            }
        }
    });
    Ok((server, owner))
}

/// Where `require` looks, shared with `frame_bench` through morf-lua.
pub use morf_lua::runtimepath_roots;
