//! A host's start: the shell's own surface and its first configure, what it
//! paints with, the first frame and the other windows.

use morf_app::{Backend, Event, PRIMARY_LAYER, WindowId, WindowKind};
use morf_desktop::Desktop;
use morf_lua::Runtime;
use morf_render::RenderEngine;
use morf_text::TextSystem;
use std::time::{Duration, Instant};

use crate::host::windows::Windows;
use crate::painter::Painter;
use crate::render_target::surface_backend;
use crate::{backdrop::*, pacing::*, paint::*, surface_actions::*, surface_layers::*, surfaces::*};

use super::{Host, slow};

/// How a host starts.
pub struct StartOptions {
    /// The output's name: where the shell's surfaces go, and what its log
    /// lines say.
    pub name: String,
    /// Paint with the GPU; without, every window is laid out and never drawn.
    pub gpu: bool,
    /// Tell the configuration what the backend, the desktop and the GPU can
    /// do (`morf.capabilities`). A runner that says it itself leaves it off.
    pub publish_capabilities: bool,
    /// A shell drawn over the whole desktop rather than on one output.
    pub desktop_canvas: bool,
    /// Say on stderr when a stage holds the output longer than a person
    /// notices (`MORF_SLOW_MS`).
    pub report_slow: bool,
}

/// What came before the shell's surface was first configured, for the
/// runner to know: where the pointer already was.
pub struct FirstFrame {
    pub early_pointer: Option<(WindowId, f64, f64)>,
}

impl Host {
    /// Starts a host for a loaded configuration on `backend`: opens the
    /// shell's own surface when the backend has not (the Wayland client opens
    /// it as it connects), waits for its first configure, paints the first
    /// frame and opens the other windows the configuration declared.
    pub fn start(
        runtime: &mut Runtime,
        mut backend: Box<dyn Backend>,
        mut desktop: Option<Desktop>,
        options: StartOptions,
    ) -> Result<Self, String> {
        let name = options.name;
        let options_report = options.report_slow;
        primary_surface_root(runtime)?;
        let layer_config = runtime.layer_surface_config();
        // A clipboard or drop read finishing on its thread rings every loop,
        // so this one wakes for its answer rather than sleeping past it. Set
        // before the first configure, since a read can start before it.
        backend.set_waker(morf_io::wake_all);
        if !backend.has_window(WindowId::Layer(PRIMARY_LAYER)) {
            let mut config = runtime_bar_config(&layer_config, &name)?;
            if options.desktop_canvas {
                config.output = None;
            }
            backend.open(WindowId::Layer(PRIMARY_LAYER), WindowKind::Layer(config))?;
        }
        open_reserve_layers(&mut *backend, &layer_config, &name)?;
        open_backdrop_layer(&mut *backend, &layer_config, &name)?;
        if let Some(desktop) = desktop.as_mut() {
            desktop.set_idle_timeouts(&runtime.idle_timeouts());
        }
        let configuring = Instant::now();
        let first = wait_for_configure(&mut *backend)?;
        slow(
            options_report,
            &name,
            "waiting for the compositor's first configure",
            configuring,
        );

        let gpu = Instant::now();
        let mut painter = if options.gpu {
            let (width, height) = backend.physical_size();
            let target = backend
                .render_target(WindowId::Layer(PRIMARY_LAYER))
                .ok_or_else(|| "the primary surface is gone".to_owned())?;
            let renderer =
                surface_backend(target, width, height).map_err(|error| error.to_string())?;
            Painter::Gpu(Box::new(RenderEngine::new(renderer)))
        } else {
            Painter::Layout(Box::new(TextSystem::new()))
        };
        if options.publish_capabilities {
            // Known only now: the protocols came with the connection, the GPU
            // with the renderer. Everything a configuration or `morf info`
            // might ask.
            let mut capabilities = capabilities_of(&*backend, desktop.as_ref(), &mut painter);
            if options.desktop_canvas {
                for (key, value) in &mut capabilities {
                    if key == "desktop_canvas" {
                        *value = "true".to_owned();
                    }
                }
            }
            runtime.set_capabilities(&capabilities);
        }
        slow(options_report, &name, "starting the GPU", gpu);
        if let Some(renderer) = painter.gpu() {
            let shaders = Instant::now();
            crate::surface_run::register_shaders(runtime, renderer)?;
            slow(options_report, &name, "building shaders", shaders);
        }
        let animating_shaders = runtime.shaders_animate();
        let started = Instant::now();
        let clock = clock_text();
        runtime
            .update_clock(&clock)
            .map_err(|error| error.to_string())?;
        apply_parent_transitions(runtime, &mut painter, &*backend)?;
        let primary_root = primary_surface_root(runtime)?;
        let painting = Instant::now();
        let layout = paint(runtime, &mut painter, &*backend, primary_root, None)?;
        slow(options_report, &name, "the first frame", painting);
        let windows_opening = Instant::now();
        let mut windows = Windows::default();
        runtime.take_window_surface_change();
        runtime.take_layer_surface_change();
        apply_backdrop(&mut *backend, &runtime.layer_surface_config(), &name);
        let _ = sync_window_surfaces(runtime, &mut *backend, &mut windows, &name)?;
        apply_service_requests(runtime, &mut *backend, desktop.as_mut());
        slow(
            options_report,
            &name,
            "opening the other surfaces",
            windows_opening,
        );

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
                pointer: first.early_pointer,
                ..PointerInput::default()
            },
            drag: None,
            primary_deferred: false,
            keyboard_changes: Vec::new(),
            fallback_tick: None,
            forced_paint: None,
            painter,
        };
        Ok(Self {
            name,
            backend,
            desktop,
            state,
            reserve: layer_config.reserve,
            clock,
            started,
            a11y: crate::surface_a11y::A11ySurfaces::default(),
            layout_complaint: None,
            motion_reported: None,
            jit_logged: None,
            follow_up: true,
            pending_streak: 0,
            containment_repaint: false,
            report_slow: options_report,
        })
    }

    /// Hands the host a new connection after [`super::Turn::Recreate`]: a
    /// new surface, so a new renderer, and every window opened again.
    pub fn replace_backend(
        &mut self,
        runtime: &mut Runtime,
        mut backend: Box<dyn Backend>,
        desktop: Option<Desktop>,
    ) -> Result<(), String> {
        if self.state.painter.gpu().is_some() {
            let (width, height) = backend.physical_size();
            let target = backend
                .render_target(WindowId::Layer(PRIMARY_LAYER))
                .ok_or_else(|| "the primary surface is gone".to_owned())?;
            let renderer =
                surface_backend(target, width, height).map_err(|error| error.to_string())?;
            self.state.painter = Painter::Gpu(Box::new(RenderEngine::new(renderer)));
            // The adapter is new, so every pipeline it held is gone with it.
            if let Some(renderer) = self.state.painter.gpu() {
                crate::surface_run::register_shaders(runtime, renderer)?;
            }
        }
        self.state.windows.clear();
        backend.set_waker(morf_io::wake_all);
        self.backend = backend;
        self.desktop = desktop;
        if let Some(desktop) = self.desktop.as_mut() {
            desktop.set_idle_timeouts(&runtime.idle_timeouts());
        }
        self.reserve = runtime.layer_surface_config().reserve;
        Ok(())
    }
}

/// Waits for the shell's surface's first configure, keeping where the
/// pointer went meanwhile.
fn wait_for_configure(backend: &mut dyn Backend) -> Result<FirstFrame, String> {
    let mut early_pointer = None;
    loop {
        let came = backend.dispatch(None)?;
        let mut any = false;
        while let Some(event) = backend.next_event() {
            any = true;
            match event {
                Event::Configure { id, .. } if id == PRIMARY_LAYER => {
                    return Ok(FirstFrame { early_pointer });
                }
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
                _ => {}
            }
        }
        // A backend that has nothing more to send (headless) will never
        // configure the surface: say so rather than wait forever.
        if !came && !any {
            return Err("the shell's surface was never configured".to_owned());
        }
    }
}

/// What this output can do, as name = value pairs.
///
/// Booleans for the protocols, because "is there screencopy here" is the
/// question; strings for the GPU, because "which one" is.
fn capabilities_of(
    client: &dyn Backend,
    desktop: Option<&Desktop>,
    painter: &mut Painter,
) -> Vec<(String, String)> {
    let info = painter
        .gpu()
        .map(|renderer| renderer.backend_mut().info().clone());
    let mut list = vec![
        (
            "gpu".to_owned(),
            info.as_ref()
                .map_or("none".to_owned(), |info| info.name.clone()),
        ),
        (
            "gpu_backend".to_owned(),
            info.as_ref()
                .map_or("none".to_owned(), |info| format!("{:?}", info.backend)),
        ),
        (
            "scale_120".to_owned(),
            client.primary_scale_120().to_string(),
        ),
    ];
    let has = |test: fn(&Desktop) -> bool| desktop.is_some_and(test);
    for (name, supported) in [
        ("desktop_canvas", false),
        ("layer_shell", client.supports_layer_shell()),
        ("layer_surfaces", client.supports_layer_surfaces()),
        ("clipboard", client.supports_clipboard()),
        ("data_control", has(Desktop::supports_data_control)),
        (
            "primary_selection",
            has(Desktop::supports_primary_selection),
        ),
        ("drag_and_drop", client.supports_drag_and_drop()),
        ("virtual_keyboard", client.supports_virtual_keyboard()),
        ("input_method", client.supports_input_method()),
        ("text_input", client.supports_text_input()),
        ("screencopy", has(Desktop::supports_screencopy)),
        ("image_capture", has(Desktop::supports_image_capture)),
        ("window_capture", has(Desktop::supports_window_capture)),
        (
            "dmabuf_capture",
            has(Desktop::supports_dmabuf_capture) && info.as_ref().is_some_and(|info| info.dmabuf),
        ),
        ("backdrop_blur", client.supports_backdrop_blur()),
        ("toplevels", has(Desktop::supports_toplevels)),
        ("toplevel_control", has(Desktop::supports_toplevel_control)),
        ("gamma_control", has(Desktop::supports_gamma_control)),
        ("idle_inhibit", client.supports_idle_inhibit()),
    ] {
        list.push((name.to_owned(), supported.to_string()));
    }
    list
}
