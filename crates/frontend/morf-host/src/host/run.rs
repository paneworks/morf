use morf_app::{LayerClient, Output};
use morf_lua::{Limits, Runtime, Screen};
use morf_render::{RenderEngine, ShaderRegistration, WgpuBackend};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::desktop::desktop_for;
use crate::{lock::*, supervisor::*, surfaces::*, wake_plan::*, workers::*};

use crate::host::turn::{Host, Links, StartOptions, Turn};
use std::os::fd::AsFd;

pub fn run_surface(start: WorkerStart, screen: Output) -> Result<(), String> {
    let name = screen
        .name
        .clone()
        .ok_or_else(|| format!("output {} has no compositor name", screen.id))?;
    let runtime_screen = Screen {
        id: screen.id,
        name: name.clone(),
        make: screen.make.clone(),
        model: screen.model.clone(),
        description: screen.description.clone(),
        position: screen.position,
        width: screen.size.map(|size| size.0),
        height: screen.size.map(|size| size.1),
        physical_size: screen.physical_size,
        scale: screen.scale,
        transform: screen.transform.to_owned(),
    };
    let mut runtime = Runtime::for_screen(
        {
            let (limits, warnings) = Limits::from_env();
            for warning in warnings {
                eprintln!("morf: {warning}");
            }
            limits
        },
        runtime_screen.clone(),
    );
    // Values the outputless runtime kept while there was no output.
    if let Some(seed) = start.seed.clone() {
        runtime.restore_reloadable_state(seed);
    }
    // Before the configuration runs, so it reads the answer from its first
    // line; the supervisor says when the duty moves later.
    runtime.set_primary(start.primary);
    let result = drive_surface(&mut runtime, &start, &name, &runtime_screen);
    // Whatever ends this output, the next runtime can start from here.
    start.handover.deposit(&mut runtime);
    result
}

fn drive_surface(
    runtime: &mut Runtime,
    start: &WorkerStart,
    name: &str,
    runtime_screen: &Screen,
) -> Result<(), String> {
    let (path, source, policy, tx, stop, commands) = (
        start.path.as_path(),
        &start.source[..],
        start.policy,
        &start.tx,
        &*start.stop,
        &start.commands,
    );
    let name = name.to_owned();
    let desktop_canvas = name == DESKTOP_CANVAS;
    if desktop_canvas {
        runtime.set_capabilities(&[("desktop_canvas".to_owned(), "true".to_owned())]);
    }
    let loading = Instant::now();
    execute_config(runtime, path, source, policy)?;
    slow(&name, "loading the configuration", loading);
    // A configuration that asks to lock the session is one client for every
    // output, not one layer per worker: the supervisor hears it and runs it
    // as that instead.
    let session_lock = runtime.layer_surface_config().session_lock;
    let _ = tx.send(SupervisorMessage::Worker(WorkerMessage::Loaded {
        output: name.clone(),
        session_lock,
        outputless: runtime.layer_surface_config().outputless,
    }));
    if session_lock {
        return await_lock(std::mem::take(runtime), path, stop, commands);
    }
    primary_surface_root(runtime)?;

    let layer_config = runtime.layer_surface_config();
    let mut bar_config = runtime_bar_config(&layer_config, &name)?;
    if desktop_canvas {
        bar_config.output = None;
    }
    let client = LayerClient::connect(bar_config).map_err(|error| error.to_string())?;
    let desktop = desktop_for(&client)?;
    tx.send(SupervisorMessage::Worker(WorkerMessage::Screens {
        output: name.clone(),
        screens: client.screens().to_vec(),
    }))
    .map_err(|_| "output supervisor stopped".to_owned())?;
    let mut host = Host::start(
        runtime,
        Box::new(client),
        Some(desktop),
        StartOptions {
            name: name.clone(),
            gpu: true,
            publish_capabilities: true,
            desktop_canvas,
            report_slow: true,
        },
    )?;
    let links = Links {
        policy,
        tx,
        commands,
        screen: runtime_screen,
    };
    let wake = morf_io::Wake::new().map_err(|error| error.to_string())?;
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        host.wait(runtime, Some(wake.as_fd()))?;
        wake.drain();
        match host.turn(runtime, Some(&links))? {
            Turn::Again => {}
            Turn::Stop => return Ok(()),
            Turn::Recreate => {
                let replacement = connect_runtime_surface(runtime, &name)?;
                let desktop = desktop_for(&replacement)?;
                tx.send(SupervisorMessage::Worker(WorkerMessage::Screens {
                    output: name.clone(),
                    screens: replacement.screens().to_vec(),
                }))
                .map_err(|_| "output supervisor stopped".to_owned())?;
                host.replace_backend(runtime, Box::new(replacement), Some(desktop))?;
            }
        }
    }
}

/// Says, on stderr, when one stage of the loop held the output longer than a
/// person notices: a configuration that blocks in a handler, a layout that
/// takes a quarter second. Silent for anything quicker; `MORF_SLOW_MS` sets
/// the threshold (default 150). Under `MORF_PROFILE`, a slow stage also says
/// whose work filled it, the costliest first; either way what the profiler
/// gathered is dropped here, so the next stage starts clean.
pub fn slow(name: &str, what: &str, since: Instant) {
    static THRESHOLD: std::sync::OnceLock<u128> = std::sync::OnceLock::new();
    let threshold = *THRESHOLD.get_or_init(|| {
        std::env::var("MORF_SLOW_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(150)
    });
    let took = since.elapsed().as_millis();
    if took >= threshold {
        eprintln!("{} morf: output {name}: {what} took {took} ms", stamp());
        for line in morf_lua::profile::report(PROFILE_LINES) {
            eprintln!("    {line}");
        }
    } else {
        morf_lua::profile::clear();
    }
}

/// How many of a slow stage's costliest pieces of work `MORF_PROFILE` names.
const PROFILE_LINES: usize = 12;

/// Builds a pipeline for every shader the configuration registered.
///
/// Once, at startup and after a device loss — never during a frame. Compiling a
/// pipeline costs tens of milliseconds, which is several frames' worth of
/// budget, and a shader is known the moment the configuration finishes loading.
pub fn register_shaders(
    runtime: &Runtime,
    renderer: &mut RenderEngine<WgpuBackend>,
) -> Result<(), String> {
    for shader in runtime.shaders() {
        renderer
            .backend_mut()
            .register_shader(ShaderRegistration {
                program: shader.program,
                wgsl: Some(&shader.wgsl),
                vertex: shader.vertex.as_deref(),
                offsets: &shader.offsets,
                uniform_size: shader.uniform_size,
                owns_coverage: shader.owns_coverage,
                effect: shader.samples_behind,
                textures: &shader.textures,
                data: &shader.data,
            })
            .map_err(|error| format!("shader pipeline: {error}"))?;
    }
    Ok(())
}

/// A worker whose configuration asked to lock the session waits here for the
/// supervisor to say whether it is the one that becomes the lock (the first
/// to ask) or stops.
pub fn await_lock(
    runtime: Runtime,
    path: &Path,
    stop: &AtomicBool,
    commands: &mpsc::Receiver<WorkerCommand>,
) -> Result<(), String> {
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        match commands.recv_timeout(Duration::from_millis(50)) {
            Ok(WorkerCommand::BecomeLock) => return crate::lock::run_lock(runtime, path),
            Ok(WorkerCommand::Call { reply, .. }) => {
                let _ = reply.send(Err("the configuration is a session lock".to_owned()));
            }
            Ok(WorkerCommand::Reload { reply, .. }) => {
                let _ = reply.send(Err("the configuration is a session lock".to_owned()));
            }
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}
