use morf_io::{IpcReply, IpcRequest, IpcServer, IpcValue as WireValue};
use morf_lua::{LogEntry, LogLevel, Runtime, Screen};
use morf_wayland::{LayerClient, ScreenInfo};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self};
use std::time::{Duration, SystemTime};

use crate::{config::*, lock::*, services::*, workers::*};

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
    let mut desired = named_screens(probe.screens())?;
    // Seeded before the first worker exists, so the very first configuration
    // load already sees every output and not only the one it draws to.
    store_outputs(probe.screens());
    drop(probe);
    if desired.is_empty() {
        return Err("compositor advertised no named outputs".to_owned());
    }
    // The file says what it is, and the only way to hear it is to run it. One
    // execution here, before any worker: a configuration that asks to be a
    // session lock is one client for every output, which is not the shape the
    // workers below have, so it is run as that instead. Every other
    // configuration is run again by its workers, the way a reload already
    // runs a candidate before trusting it.
    {
        let mut first = Runtime::default();
        execute_config(&mut first, &path, &source, policy)?;
        if first.layer_surface_config().session_lock {
            return run_lock(first, &path);
        }
    }
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
    let mut workers = BTreeMap::new();
    let mut daemon_logs = Vec::new();
    reconcile_workers(
        &mut workers,
        &desired,
        Arc::clone(&path),
        Arc::clone(&source),
        policy,
        &tx,
    );

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
                desired = named_screens(&screens)?;
                reconcile_workers(
                    &mut workers,
                    &desired,
                    Arc::clone(&path),
                    Arc::clone(&source),
                    policy,
                    &tx,
                );
            }
            Ok(SupervisorMessage::Worker(WorkerMessage::Screens { .. })) => {}
            Ok(SupervisorMessage::Worker(WorkerMessage::Failed { output, error })) => {
                stop_workers(workers);
                return Err(format!("output {output}: {error}"));
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

pub(crate) fn named_screens(
    screens: &[ScreenInfo],
) -> Result<BTreeMap<String, ScreenInfo>, String> {
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
pub(crate) static OUTPUTS: std::sync::Mutex<Vec<ScreenInfo>> = std::sync::Mutex::new(Vec::new());

/// Records the compositor's output list, reporting whether it changed.
pub(crate) fn store_outputs(screens: &[ScreenInfo]) -> bool {
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
pub(crate) fn lua_screens(screens: &[ScreenInfo]) -> Vec<Screen> {
    screens.iter().map(lua_screen).collect()
}

/// An output with no compositor name cannot be addressed by a configuration,
/// but it still occupies the desktop, so it is described with an empty name
/// rather than dropped from the list.
pub(crate) fn lua_screen(screen: &ScreenInfo) -> Screen {
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
    let roots = runtimepath_roots(path, policy.external_roots);
    // Applied before any Lua runs, so a configuration can measure itself
    // against the whole monitor layout while it loads. Index 1 of
    // `morf.screens` stays this runtime's own output.
    runtime.set_screens(&known_outputs());
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
