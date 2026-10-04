mod stall;
mod turn;

use morf_app::Backend;
use morf_app::{Event, LayerClient, Output, PRIMARY_LAYER};
use morf_lua::{Limits, Runtime, Screen};
use morf_render::{RenderEngine, ShaderRegistration, WgpuBackend};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::desktop::desktop_for;
use crate::host::windows::Windows;
use crate::render_target::{primary_target, surface_backend};
use crate::{
    backdrop::*, lock::*, pacing::*, paint::*, supervisor::*, surface_actions::*,
    surface_layers::*, surfaces::*, wake_plan::*, workers::*,
};

use stall::{advance_without_callbacks, motion_deadline};
use turn::turn_loop;

/// What this output can do, as name = value pairs.
///
/// Booleans for the protocols, because "is there screencopy here" is the
/// question; strings for the GPU, because "which one" is.
fn capabilities_of(
    client: &dyn Backend,
    desktop: &morf_desktop::Desktop,
    renderer: &mut RenderEngine<WgpuBackend>,
) -> Vec<(String, String)> {
    let info = renderer.backend_mut().info();
    let mut list = vec![
        ("gpu".to_owned(), info.name.clone()),
        ("gpu_backend".to_owned(), format!("{:?}", info.backend)),
        (
            "scale_120".to_owned(),
            client.primary_scale_120().to_string(),
        ),
    ];
    for (name, supported) in [
        ("desktop_canvas", false),
        ("layer_shell", client.supports_layer_shell()),
        ("layer_surfaces", client.supports_layer_surfaces()),
        ("clipboard", client.supports_clipboard()),
        ("data_control", desktop.supports_data_control()),
        ("primary_selection", desktop.supports_primary_selection()),
        ("drag_and_drop", client.supports_drag_and_drop()),
        ("virtual_keyboard", client.supports_virtual_keyboard()),
        ("input_method", client.supports_input_method()),
        ("text_input", client.supports_text_input()),
        ("screencopy", desktop.supports_screencopy()),
        ("image_capture", desktop.supports_image_capture()),
        ("window_capture", desktop.supports_window_capture()),
        (
            "dmabuf_capture",
            desktop.supports_dmabuf_capture() && info.dmabuf,
        ),
        ("backdrop_blur", client.supports_backdrop_blur()),
        ("toplevels", desktop.supports_toplevels()),
        ("toplevel_control", desktop.supports_toplevel_control()),
        ("gamma_control", desktop.supports_gamma_control()),
        ("idle_inhibit", client.supports_idle_inhibit()),
    ] {
        list.push((name.to_owned(), supported.to_string()));
    }
    list
}

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
    let mut client = LayerClient::connect(bar_config).map_err(|error| error.to_string())?;
    let mut desktop = desktop_for(&client)?;
    // A clipboard or drop read finishing on its thread rings every loop, so
    // this one wakes for its answer rather than sleeping past it. Set before
    // the first configure, since a read can start before it.
    client.set_waker(morf_io::wake_all);
    open_reserve_layers(&mut client, &layer_config, &name)?;
    open_backdrop_layer(&mut client, &layer_config, &name)?;
    // What the reservers were last built from. A reserver is a separate surface
    // per edge, so a thickness change is the one part of `morf.surface` that
    // still has to rebuild something, and it must not rebuild on every
    // unrelated margin the configuration animates.
    let reserve = layer_config.reserve;

    desktop.set_idle_timeouts(&runtime.idle_timeouts());
    tx.send(SupervisorMessage::Worker(WorkerMessage::Screens {
        output: name.clone(),
        screens: client.screens().to_vec(),
    }))
    .map_err(|_| "output supervisor stopped".to_owned())?;
    let configuring = Instant::now();
    let mut early_pointer = None;
    'configured: loop {
        client
            .blocking_dispatch()
            .map_err(|error| error.to_string())?;
        while let Some(event) = client.next_event() {
            match event {
                Event::Configure { id, .. } if id == PRIMARY_LAYER => break 'configured,
                Event::Closed { id } if id == PRIMARY_LAYER => {
                    return Err(crate::supervisor::SURFACE_CLOSED.to_owned());
                }
                Event::PointerMotion { surface, x, y } => {
                    early_pointer = Some((surface, x, y));
                }
                Event::PointerLeave { surface } => {
                    if early_pointer.is_some_and(|(role, _, _)| role == surface) {
                        early_pointer = None;
                    }
                }
                Event::Configure { .. }
                | Event::Closed { .. }
                | Event::Scale { .. }
                | Event::AuxScale { .. }
                | Event::ShortcutsInhibited { .. }
                | Event::Clipboard { .. }
                | Event::OfferRead { .. }
                | Event::DragEnter { .. }
                | Event::DragMotion { .. }
                | Event::DragLeave { .. }
                | Event::Drop { .. }
                | Event::DragSourceEnded { .. }
                | Event::KeyboardFocus { .. }
                | Event::SurfaceKeyboard { .. }
                | Event::SurfacePointer { .. }
                | Event::InputMethod(_)
                | Event::TextInput(_)
                | Event::Frame { .. }
                | Event::PointerButton { .. }
                | Event::PointerAxis { .. }
                | Event::TouchDown { .. }
                | Event::TouchMotion { .. }
                | Event::TouchUp { .. }
                | Event::TouchCancel
                | Event::Key { .. }
                | Event::Screens(_)
                | Event::PopupConfigure { .. }
                | Event::PopupFrame { .. }
                | Event::PopupDone { .. }
                | Event::ToplevelConfigure { .. }
                | Event::ToplevelFrame { .. }
                | Event::ToplevelClose { .. }
                | Event::SessionLocked
                | Event::SessionLockFinished
                | Event::SessionLockConfigure { .. }
                | Event::SessionLockSurfaceRemoved { .. }
                | Event::SessionLockFrame { .. } => {}
            }
        }
    }
    slow(
        &name,
        "waiting for the compositor's first configure",
        configuring,
    );
    let gpu = Instant::now();
    let (width, height) = client.physical_size();
    let backend = surface_backend(primary_target(&client)?, width, height)
        .map_err(|error| error.to_string())?;
    let mut renderer = RenderEngine::new(backend);
    // Known only now: the protocols came with the connection, the GPU with
    // the renderer. Everything a configuration or `morf info` might ask.
    let mut capabilities = capabilities_of(&client, &desktop, &mut renderer);
    if desktop_canvas {
        for (key, value) in &mut capabilities {
            if key == "desktop_canvas" {
                *value = "true".to_owned();
            }
        }
    }
    runtime.set_capabilities(&capabilities);
    slow(&name, "starting the GPU", gpu);
    let shaders = Instant::now();
    register_shaders(runtime, &mut renderer)?;
    slow(&name, "building shaders", shaders);
    let animating_shaders = runtime.shaders_animate();
    let started = Instant::now();
    let clock = clock_text();
    runtime
        .update_clock(&clock)
        .map_err(|error| error.to_string())?;
    apply_parent_transitions(runtime, &mut renderer, &client)?;
    let primary_root = primary_surface_root(runtime)?;
    let first = Instant::now();
    let layout = paint(runtime, &mut renderer, &client, primary_root, None)?;
    slow(&name, "the first frame", first);
    let windows_opening = Instant::now();
    let mut windows = Windows::default();
    runtime.take_window_surface_change();
    runtime.take_layer_surface_change();
    apply_backdrop(&mut client, &runtime.layer_surface_config(), &name);
    let _ = sync_window_surfaces(runtime, &mut client, &mut windows, &name)?;
    apply_service_requests(runtime, &mut client, &mut desktop);
    slow(&name, "opening the other surfaces", windows_opening);

    let state = SurfaceEventState {
        layout,
        primary_root,
        windows,
        animating_shaders,
        last_frame: None,
        pacer: FramePacer::new(),
        // Until a callback says otherwise, assume the commonest refresh.
        refresh: Duration::from_micros(16_667),
        input: PointerInput {
            pointer: early_pointer,
            ..PointerInput::default()
        },
        drag: None,
        primary_deferred: false,
        keyboard_changes: Vec::new(),
        fallback_tick: None,
        forced_paint: None,
    };
    turn_loop(
        runtime,
        start,
        name,
        runtime_screen,
        client,
        desktop,
        renderer,
        reserve,
        clock,
        started,
        state,
    )
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
