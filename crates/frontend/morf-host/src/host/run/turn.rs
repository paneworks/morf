//! One output's loop: each turn sleeps until something is due, runs the
//! services, commands and events that came, and paints what changed.

use morf_app::{Event, LayerClient, PRIMARY_LAYER};
use morf_desktop::Desktop;
use morf_lua::{Runtime, Screen, SurfaceReserve};
use morf_render::{RenderEngine, WgpuBackend};
use std::os::fd::AsFd;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::desktop::{desktop_for, dispatch_desktop};
use crate::host::windows::Kind;
use crate::render_target::{primary_target, surface_backend};
use crate::{
    backdrop::*, capture::*, lock::*, paint::*, services::*, surface_actions::*, surface_events::*,
    surface_layers::*, surface_pointer::answer_new_containment, surfaces::*, wake_plan::*,
    workers::*,
};
use morf_app::Backend as _;
use morf_app::WindowId;

use super::{advance_without_callbacks, motion_deadline, register_shaders, slow};

/// One output's loop, from its first frame on: sleep until something is due,
/// run what came, and paint what changed. Returns only when told to stop, or
/// when the output is gone.
#[allow(clippy::too_many_arguments)]
pub(super) fn turn_loop(
    runtime: &mut Runtime,
    start: &WorkerStart,
    name: String,
    runtime_screen: &Screen,
    mut client: LayerClient,
    mut desktop: Desktop,
    mut renderer: RenderEngine<WgpuBackend>,
    mut reserve: SurfaceReserve,
    mut clock: String,
    started: Instant,
    mut state: SurfaceEventState,
) -> Result<(), String> {
    let (policy, tx, stop, commands) = (start.policy, &start.tx, &*start.stop, &start.commands);
    let wake = morf_io::Wake::new().map_err(|error| error.to_string())?;
    let mut a11y = crate::surface_a11y::A11ySurfaces::default();
    let mut layout_complaint: Option<Instant> = None;
    let mut motion_reported: Option<Instant> = None;
    let mut jit_logged: Option<Instant> = None;
    // Whether the last turn handled anything -- an event, a command -- whose
    // handlers may have left work that only the checks at the top of a turn
    // pick up (a popup to open, a reload asked for). One more turn, at once.
    let mut follow_up = true;
    let mut pending_streak = 0;
    // A node first asked for its `contains_pointer` last turn, whose answer
    // changed what bindings drew: this turn paints it.
    let mut containment_repaint = false;
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        // Until the compositor sends something, a service thread rings the
        // alarm, or the first thing that comes due on the clock. Nothing else:
        // an idle shell sleeps until one of those.
        let sleep = Sleep::plan_with(
            runtime,
            std::mem::take(&mut follow_up) || client.has_queued_events(),
            motion_deadline(runtime, &client, &state),
            &mut pending_streak,
        );
        let slept = Instant::now();
        let woke = client
            .wait_for(sleep.timeout(), Some(wake.as_fd()))
            .map_err(|error| error.to_string())?;
        let desktop_repaint = dispatch_desktop(runtime, &mut desktop, Some(&mut renderer))?;
        wake.drain();
        log_wake(&name, woke, &sleep, slept);
        // Now and then, what keeps the loop drawing, when anything does.
        if wake_log_wanted()
            && runtime.has_motion()
            && motion_reported.is_none_or(|at: Instant| at.elapsed() >= Duration::from_secs(2))
        {
            motion_reported = Some(Instant::now());
            for line in runtime.motion_report(8) {
                eprintln!("morf: output {name}: moving: {line}");
            }
        }
        // Before the services, so a callback reading the time reads it as it
        // is now, not as it was when the loop last woke.
        let next_clock = clock_text();
        let mut repaint = std::mem::take(&mut containment_repaint) | desktop_repaint;
        if next_clock != clock {
            clock = next_clock;
            repaint |= runtime
                .update_clock(&clock)
                .map_err(|error| error.to_string())?;
        }
        // `MORF_JIT_LOG`: what the native Lua tier has run, every ten seconds.
        if jit_log_wanted()
            && jit_logged.is_none_or(|at: Instant| at.elapsed() >= Duration::from_secs(10))
        {
            jit_logged = Some(Instant::now());
            if let Some(report) = runtime.jit_report() {
                eprintln!("morf: output {name}: jit: {report}");
            }
        }
        let polling = Instant::now();
        repaint |= runtime.poll_services();
        slow(&name, "services, timers and callbacks", polling);
        let mut recreate_surface = false;
        while let Ok(command) = commands.try_recv() {
            follow_up = true;
            let started_command = Instant::now();
            let update = handle_worker_command(runtime, Some(runtime_screen), policy, command);
            slow(&name, "an IPC call", started_command);
            repaint |= update.repaint;
            recreate_surface |= update.recreate_surface;
            if update.reset_input {
                state.input.reset();
            }
            if update.refresh_idle {
                desktop.set_idle_timeouts(&runtime.idle_timeouts());
            }
            if update.reloaded {
                // Node handles and layout revisions belong to a single runtime.
                // A new configuration may reuse an old handle for an unrelated
                // node, or build an entirely different tree (a theme switch).
                state.primary_root = primary_surface_root(runtime)?;
                state.layout.invalidate_scene();
                state.animating_shaders = runtime.shaders_animate();
                register_shaders(runtime, &mut renderer)?;
                for surface in state.windows.values_mut() {
                    surface.layout = None;
                }
                let _ = desktop.reset_gamma(None);
                // What to do once every output is gone may have changed.
                tx.send(SupervisorMessage::Worker(WorkerMessage::Outputless {
                    output: name.clone(),
                    wanted: runtime.layer_surface_config().outputless,
                }))
                .map_err(|_| "output supervisor stopped".to_owned())?;
            }
        }
        if recreate_surface {
            let replacement = connect_runtime_surface(runtime, &name)?;
            let (width, height) = replacement.physical_size();
            let backend = surface_backend(primary_target(&replacement)?, width, height)
                .map_err(|error| error.to_string())?;
            renderer = RenderEngine::new(backend);
            // The adapter is new, so every pipeline it held is gone with it.
            register_shaders(runtime, &mut renderer)?;
            state.windows.clear();
            client = replacement;
            desktop = desktop_for(&client)?;
            desktop.set_idle_timeouts(&runtime.idle_timeouts());
            client.set_waker(morf_io::wake_all);
            reserve = runtime.layer_surface_config().reserve;
            tx.send(SupervisorMessage::Worker(WorkerMessage::Screens {
                output: name.clone(),
                screens: client.screens().to_vec(),
            }))
            .map_err(|_| "output supervisor stopped".to_owned())?;
        }
        if let Some(hard) = runtime.take_reload_request() {
            tx.send(SupervisorMessage::Reload { hard })
                .map_err(|_| "output supervisor stopped".to_owned())?;
        }
        if runtime.quit_requested() {
            // Told once, and then this output stops driving frames. The
            // supervisor takes the others down; returning here rather than
            // waiting for it keeps this thread from painting a shell that is
            // already leaving.
            tx.send(SupervisorMessage::Quit)
                .map_err(|_| "output supervisor stopped".to_owned())?;
            return Ok(());
        }
        apply_idle_inhibit(runtime, &mut client);
        apply_idle_timeouts(runtime, &mut desktop);
        apply_shortcuts_inhibit(runtime, &mut client);
        if let Some(enabled) = runtime.take_watch_files_change() {
            tx.send(SupervisorMessage::WatchFiles(enabled))
                .map_err(|_| "output supervisor stopped".to_owned())?;
        }
        if runtime.take_layer_surface_change() {
            // Layer shell allows all of this on a mapped surface, so the shell's
            // own geometry follows an assignment to `morf.surface` without a
            // reconnect. The configure this provokes resizes the backend in
            // place; nothing here tears the renderer down.
            let config = runtime.layer_surface_config();
            client
                .set_layer_geometry(PRIMARY_LAYER, &runtime_bar_config(&config, &name)?)
                .map_err(|error| error.to_string())?;
            if config.reserve != reserve {
                reserve = config.reserve;
                open_reserve_layers(&mut client, &config, &name)?;
            }
            apply_primary_opaque(runtime, &client);
            apply_backdrop(&mut client, &config, &name);
            // The mask lives in the same configuration and is re-derived when
            // the surface paints, so the new geometry owes one frame even when
            // the compositor has no configure to send back.
            repaint = true;
        }
        if runtime.take_window_surface_change() {
            // The only thing that can move the primary root.
            state.primary_root = primary_surface_root_keeping(runtime, state.primary_root)?;
            repaint |= sync_window_surfaces(runtime, &mut client, &mut state.windows, &name)?;
        }
        apply_service_requests(runtime, &mut client, &mut desktop);
        while let Some(event) = client.next_event() {
            follow_up = true;
            let handling = Instant::now();
            let what = event_kind(&event);
            let handled = handle_surface_event(
                runtime,
                &mut renderer,
                &mut client,
                &mut desktop,
                &mut state,
                event,
                tx,
                &name,
            );
            slow(&name, what, handling);
            match handled {
                Ok(painted) => repaint |= painted,
                // A scene that is mid-change (a view that removed a node
                // whose parent still lists it) fails the layout for this
                // event; the event is dropped and the shell stays up, and
                // the next paint sees whatever the scene has become.
                Err(error) if is_layout_error(&error) => {
                    complain_layout(&name, &error, &mut layout_complaint);
                    repaint = true;
                }
                Err(error) => return Err(error),
            }
        }
        apply_service_requests(runtime, &mut client, &mut desktop);
        apply_capture_releases(runtime, &mut renderer);
        apply_window_surface_actions(runtime, &client, &state.windows);
        advance_without_callbacks(runtime, &client, &mut state)?;
        // Motion with nothing to drive it: started where no turn noticed (a
        // binding flushed after an animation's `on_finished`, say) while no
        // frame callback is outstanding. The callbacks are its clock, and
        // only a paint asks for one -- without this it waits, frozen, for
        // whatever paints next: a pill that stays lit, a swell that shows
        // seconds late.
        if !repaint && client.layer_frame_wait(PRIMARY_LAYER).is_none() && runtime.has_motion() {
            repaint = true;
        }
        // A paint owed for longer than a stall is made without the callback.
        let owed = owed_paint_due(
            state.primary_deferred,
            client.layer_frame_wait(PRIMARY_LAYER),
            state.refresh,
            state.forced_paint,
            Instant::now(),
        );
        if owed {
            state.primary_deferred = false;
            state.forced_paint = Some(Instant::now());
            repaint = true;
        }
        // A surface still waiting for its last frame callback is not
        // presented to again: under FIFO the present blocks until that
        // callback, and a surface the compositor is not showing (a fallback
        // toplevel under another, in cage) never gets one -- which froze this
        // whole output, every other surface and IPC with it. The callback,
        // when it comes, makes the paint.
        if repaint && !owed && client.layer_frame_wait(PRIMARY_LAYER).is_some() {
            state.primary_deferred = true;
            for surface in state
                .windows
                .of_kind_mut(Kind::Layer)
                .map(|(_, surface)| surface)
            {
                surface.needs_paint |= surface.updates_enabled;
            }
            repaint = false;
            // The layer surfaces are not held by the primary's callback;
            // each paints when its own allows.
            for surface in state
                .windows
                .of_kind_mut(Kind::Layer)
                .map(|(_, surface)| surface)
                .filter(|surface| surface.updates_enabled)
            {
                paint_layer_surface(runtime, &client, surface)?;
            }
        }
        if repaint {
            let painted = Instant::now();
            renderer
                .backend_mut()
                .set_elapsed(started.elapsed().as_secs_f32());
            // Before anything is drawn, tell the renderer what died. Its caches
            // are keyed on nodes and it has no other way to find out; without
            // this a shaped text buffer survives every view switch for the life
            // of the process.
            let removed = runtime.take_removed_nodes();
            if !removed.is_empty() {
                renderer.backend_mut().forget_nodes(&removed);
                for renderer in state
                    .windows
                    .values_mut()
                    .filter_map(|surface| surface.renderer.as_mut())
                {
                    renderer.backend_mut().forget_nodes(&removed);
                }
            }
            apply_parent_transitions(runtime, &mut renderer, &client)?;
            let painting = Instant::now();
            let painted_frame = paint(
                runtime,
                &mut renderer,
                &client,
                state.primary_root,
                Some(&mut state.layout),
            );
            slow(&name, "a frame", painting);
            // Skipped for want of a buffer: owed, and painted on the next
            // callback (or when the callback is overdue).
            if renderer.backend_mut().take_skipped() {
                state.primary_deferred = true;
                // Nothing committed, so no callback may be coming: ask for
                // one, or the owed paint waits for whatever paints next.
                if client.layer_frame_wait(PRIMARY_LAYER).is_none() {
                    client.request_frame(WindowId::Layer(PRIMARY_LAYER));
                    client.commit(WindowId::Layer(PRIMARY_LAYER));
                }
            }
            match painted_frame {
                Ok(layout) => state.layout = layout,
                Err(error) if is_layout_error(&error) => {
                    complain_layout(&name, &error, &mut layout_complaint);
                    continue;
                }
                Err(error) => return Err(error),
            }
            for surface in state
                .windows
                .of_kind_mut(Kind::Popup)
                .map(|(_, surface)| surface)
                .filter(|surface| surface.updates_enabled)
            {
                paint_popup_surface(runtime, &client, surface)?;
            }
            for surface in state
                .windows
                .of_kind_mut(Kind::Toplevel)
                .map(|(_, surface)| surface)
                .filter(|surface| surface.updates_enabled)
            {
                paint_floating_surface(runtime, &client, surface)?;
            }
            for surface in state
                .windows
                .of_kind_mut(Kind::Layer)
                .map(|(_, surface)| surface)
                .filter(|surface| surface.updates_enabled)
            {
                paint_layer_surface(runtime, &client, surface)?;
            }
            // What this frame actually cost, which is what the next one is
            // paced against.
            state.pacer.observed(painted.elapsed());
        }
        // Layout observation can rebuild a responsive authentication tree
        // during paint. Draw that new tree before answering its pointer
        // watchers, otherwise the first pointer position is consumed against
        // removed nodes and monitor ownership stays wrong until the next move.
        //
        // A paint held back for the frame callback cannot happen this turn,
        // so the tree stays newer than the layout until the callback comes:
        // turning again at once spun the loop for the whole wait (10-40 ms
        // a frame on a busy GPU), thousands of turns a second while anything
        // ticked. The paint is owed instead; the callback, or the stall
        // deadline when none comes, makes it.
        if runtime.scene().layout_revision_of(state.primary_root) != state.layout.revision {
            containment_repaint = true;
            if client.layer_frame_wait(PRIMARY_LAYER).is_some() {
                state.primary_deferred = true;
            } else {
                follow_up = true;
            }
            continue;
        }
        // After the paints, so a node built this turn is laid out by now.
        let layouts = LayerLayouts {
            layout: &state.layout,
            windows: &state.windows,
        };
        if answer_new_containment(runtime, &state.input, &layouts) {
            containment_repaint = true;
            follow_up = true;
        }
        // A screen reader's tree and requests, once the layouts are fresh.
        for (root, focused) in state.keyboard_changes.drain(..) {
            a11y.window_focus(root, focused);
        }
        if a11y.turn(runtime, &state, &name, repaint) {
            follow_up = true;
        }
    }
}

/// What an event is, for `slow`.
fn event_kind(event: &Event) -> &'static str {
    match event {
        Event::PointerMotion { .. } => "pointer motion",
        Event::PointerButton { .. } => "a click",
        Event::PointerAxis { .. } => "a scroll",
        Event::Key { .. } => "a key",
        Event::Configure { .. } => "a configure",
        Event::Frame { .. } => "a frame callback",
        _ => "an event",
    }
}

/// Whether an error is the layout's, from a scene that is mid-change, and
/// not the compositor's or the GPU's. A layout error is a frame's, not the
/// output's: the next paint starts from the scene as it is then.
fn is_layout_error(error: &str) -> bool {
    error.contains("scene layout error") || error.contains("stale scene node handle")
}

/// Says what the layout could not do, at most once a second, so a scene
/// that stays broken does not flood the log.
fn complain_layout(name: &str, error: &str, last: &mut Option<Instant>) {
    if last.is_some_and(|at| at.elapsed() < Duration::from_secs(1)) {
        return;
    }
    *last = Some(Instant::now());
    eprintln!("morf: output {name}: frame skipped: {error}");
}

/// Whether `MORF_JIT_LOG` asks for the native Lua tier's counters.
fn jit_log_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| {
        std::env::var_os("MORF_JIT_LOG").is_some_and(|value| !value.is_empty() && value != "0")
    })
}
