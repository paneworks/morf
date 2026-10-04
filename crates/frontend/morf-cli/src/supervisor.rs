use morf_io::{IpcReply, IpcRequest, IpcServer, IpcValue as WireValue};
use morf_lua::{LogEntry, LogLevel, Runtime, Screen};
use morf_app::{LayerClient, Output};
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self};
use std::time::{Duration, Instant, SystemTime};

use crate::{config::*, lock::*, outputless::*, services::*, workers::*};

/// What a worker ends with when the compositor closes its surface -- which is
/// what a compositor does to every surface on an output it switches off or
/// loses, often before it says the output is gone.
pub(crate) const SURFACE_CLOSED: &str = "layer surface was closed";

/// Cage presents one fullscreen view across its output layout. A greeter
/// there needs one controller/canvas, rather than overlapping output workers.
pub(crate) const DESKTOP_CANVAS: &str = "@desktop-canvas";

/// Closed surfaces taken in a minute before the shell stops: one closed on
/// every output the shell is given would otherwise be asked for forever.
const CLOSURES_PER_MINUTE: usize = 5;

/// What the supervisor does when a worker fails.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FailureStep {
    /// Something is wrong with the shell: stop it.
    Stop,
    /// Its output went away: let that worker go and ask for the outputs again.
    Probe,
}

/// Decides a worker's failure. A closed surface is an output switched off
/// or unplugged -- the shell's last one included, when Hyprland switches to
/// its fallback output -- and the shell stopping for it left the machine
/// with no shell at all, and nothing to light a screen again.
pub(crate) fn failure_step(
    error: &str,
    closures: &mut VecDeque<Instant>,
    now: Instant,
) -> FailureStep {
    if error != SURFACE_CLOSED {
        return FailureStep::Stop;
    }
    while closures
        .front()
        .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(60))
    {
        closures.pop_front();
    }
    closures.push_back(now);
    if closures.len() > CLOSURES_PER_MINUTE {
        FailureStep::Stop
    } else {
        FailureStep::Probe
    }
}

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

pub(crate) fn supervise(path: PathBuf, source: Vec<u8>, policy: LoadPolicy) -> Result<(), String> {
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
                .send(SupervisorMessage::Reload { hard: false })
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
                if failure_step(&error, &mut closures, Instant::now()) == FailureStep::Stop {
                    stop_workers(workers);
                    return Err(format!("output {output}: {error}"));
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
            Ok(SupervisorMessage::Reload { hard }) => {
                match fs::read(path.as_ref()) {
                    Ok(bytes) => {
                        source = Arc::from(bytes);
                        for (output, worker) in &workers {
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
pub(crate) enum LoadedStep {
    /// A shell: keep going.
    Run,
    /// A shell on a display whose socket another one holds: stop.
    Refuse,
    /// A session lock: that worker becomes the lock, the others stop.
    Lock,
}

pub(crate) fn loaded_step(session_lock: bool, socket_taken: bool) -> LoadedStep {
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

pub(crate) fn named_screens(
    screens: &[Output],
) -> Result<BTreeMap<String, Output>, String> {
    screens
        .iter()
        .map(|screen| {
            screen
                .name
                .clone()
                .map(|name| (name, screen.clone()))
                .ok_or_else(|| format!("output {} has no compositor name", screen.id))
        })
        .collect()
}

/// Every output the compositor currently advertises, in the order it advertised
/// them.
///
/// One morf process drives every output, one worker thread each, so the output
/// topology is a fact about the process rather than per-worker state. The
/// supervisor is the only writer: it seeds this from its probe connection
/// before the first worker starts and refreshes it whenever a worker reports a
/// change. Workers read it when they load a configuration, which is what lets
/// `morf.screens` describe more than the one output a worker draws to.
pub(crate) static OUTPUTS: std::sync::Mutex<Vec<Output>> = std::sync::Mutex::new(Vec::new());

/// Records the compositor's output list, reporting whether it changed.
pub(crate) fn store_outputs(screens: &[Output]) -> bool {
    let mut outputs = OUTPUTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if outputs.as_slice() == screens {
        return false;
    }
    outputs.clear();
    outputs.extend_from_slice(screens);
    true
}

/// The recorded output list in the shape `morf.screens` is built from.
pub(crate) fn known_outputs() -> Vec<Screen> {
    let outputs = OUTPUTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    lua_screens(&outputs)
}

/// Converts compositor output descriptions into the Lua-facing shape, keeping
/// the order the compositor advertised them in.
pub(crate) fn lua_screens(screens: &[Output]) -> Vec<Screen> {
    screens.iter().map(lua_screen).collect()
}

/// An output with no compositor name cannot be addressed by a configuration,
/// but it still occupies the desktop, so it is described with an empty name
/// rather than dropped from the list.
pub(crate) fn lua_screen(screen: &Output) -> Screen {
    Screen {
        id: screen.id,
        name: screen.name.clone().unwrap_or_default(),
        make: screen.make.clone(),
        model: screen.model.clone(),
        description: screen.description.clone(),
        position: screen.position,
        width: screen.size.map(|size| size.0),
        height: screen.size.map(|size| size.1),
        physical_size: screen.physical_size,
        scale: screen.scale,
        transform: screen.transform.to_owned(),
    }
}

/// Where `require` looks, shared with `frame_bench` through morf-lua.
pub(crate) use morf_lua::runtimepath_roots;

pub(crate) fn execute_config(
    runtime: &mut Runtime,
    path: &Path,
    source: &[u8],
    policy: LoadPolicy,
) -> Result<(), String> {
    execute_config_on(runtime, path, source, policy, &known_outputs())
}

/// As [`execute_config`], with `morf.screens` given `screens` rather than
/// the recorded outputs: the outputless runtime has none by definition.
pub(crate) fn execute_config_on(
    runtime: &mut Runtime,
    path: &Path,
    source: &[u8],
    policy: LoadPolicy,
    screens: &[Screen],
) -> Result<(), String> {
    let roots = runtimepath_roots(path, policy.external_roots);
    // Applied before any Lua runs, so a configuration can measure itself
    // against the whole monitor layout while it loads. Index 1 of
    // `morf.screens` stays this runtime's own output.
    if runtime
        .capabilities()
        .iter()
        .any(|value| value == "desktop_canvas=true")
    {
        runtime.replace_screens(screens);
    } else {
        runtime.set_screens(screens);
    }
    runtime.set_module_roots(roots.clone());
    runtime.set_shell_root(
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    );
    for plugin in policy
        .plugins
        .then(|| runtime_scripts(&roots, "plugin"))
        .into_iter()
        .flatten()
    {
        match fs::read(&plugin) {
            Ok(source) => {
                if let Err(error) = runtime.execute(&plugin.to_string_lossy(), &source) {
                    eprintln!("morf: plugin {}: {error}", plugin.display());
                }
            }
            Err(error) => eprintln!("morf: plugin {}: {error}", plugin.display()),
        }
    }
    runtime
        .execute(&path.to_string_lossy(), source)
        .map_err(|error| error.to_string())?;
    for after in policy
        .plugins
        .then(|| runtime_scripts(&roots, "after/plugin"))
        .into_iter()
        .flatten()
    {
        match fs::read(&after) {
            Ok(source) => {
                if let Err(error) = runtime.execute(&after.to_string_lossy(), &source) {
                    eprintln!("morf: after plugin {}: {error}", after.display());
                }
            }
            Err(error) => eprintln!("morf: after plugin {}: {error}", after.display()),
        }
    }
    Ok(())
}

pub(crate) fn runtime_scripts(roots: &[PathBuf], directory: &str) -> Vec<PathBuf> {
    let mut scripts = Vec::new();
    for root in roots {
        let mut found = Vec::new();
        collect_lua_scripts(&root.join(directory), &mut found);
        found.sort();
        for path in found {
            if !scripts.contains(&path) {
                scripts.push(path);
            }
        }
    }
    scripts
}

pub(crate) fn collect_lua_scripts(path: &Path, scripts: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_lua_scripts(&path, scripts);
        } else if path.extension().and_then(|value| value.to_str()) == Some("lua") {
            scripts.push(path);
        }
    }
}

/// How long the files must be quiet after a change before it is acted on:
/// a save, a `git checkout`, a formatter touching every file are one reload.
pub(crate) const RELOAD_SETTLE: Duration = Duration::from_millis(50);
/// The longest a stream of changes can put a reload off.
const RELOAD_SETTLE_MAX: Duration = Duration::from_secs(1);

/// Calls `changed` whenever a `.lua` file under `roots` is written, made,
/// moved or removed, while `enabled` says so; returns when `changed` says to
/// stop, or when the roots cannot be watched at all.
///
/// The roots are watched recursively through the shared inotify thread, so
/// this sleeps until something under them happens. Every event is only a
/// reason to look: once the files have been quiet for [`RELOAD_SETTLE`], the
/// `.lua` snapshot is taken again and compared, and only a difference counts
/// -- an editor's swap file, a settings file written beside the
/// configuration, a change made while watching was off, are not reloads.
pub(crate) fn follow_lua_files(
    roots: &[PathBuf],
    enabled: &AtomicBool,
    mut changed: impl FnMut(&BTreeMap<PathBuf, (u64, SystemTime)>) -> bool,
) {
    let watches = roots
        .iter()
        .filter_map(|root| {
            morf_io::Watch::new(root, morf_io::WatchOptions { recursive: true }).ok()
        })
        .collect::<Vec<_>>();
    if watches.is_empty() {
        return;
    }
    let mut snapshot = lua_snapshot(roots);
    loop {
        morf_io::wait_any(&watches, None);
        let started = std::time::Instant::now();
        loop {
            for watch in &watches {
                watch.drain();
            }
            if started.elapsed() >= RELOAD_SETTLE_MAX
                || !morf_io::wait_any(&watches, Some(RELOAD_SETTLE))
            {
                break;
            }
        }
        for watch in &watches {
            watch.drain();
        }
        let next = lua_snapshot(roots);
        if next == snapshot {
            continue;
        }
        snapshot = next;
        if enabled.load(Ordering::Acquire) && !changed(&snapshot) {
            return;
        }
    }
}

pub(crate) fn lua_snapshot(roots: &[PathBuf]) -> BTreeMap<PathBuf, (u64, SystemTime)> {
    let mut snapshot = BTreeMap::new();
    let mut pending = roots.to_vec();
    while let Some(path) = pending.pop() {
        let Ok(entries) = fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(entry.path());
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("lua") {
                continue;
            }
            if let Ok(metadata) = entry.metadata() {
                snapshot.insert(
                    path,
                    (
                        metadata.len(),
                        metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    ),
                );
            }
        }
    }
    snapshot
}
