//! The host: one output's windows on a backend, and the turn of its loop.
//!
//! The same code runs a shell on a compositor and a configuration under
//! `morf check` and `morf test`: what differs is the backend under it (the
//! Wayland client, or the headless one with its virtual outputs, seat and
//! clock), whether it paints with a GPU or only lays out, and whether a
//! supervisor is on the other end of [`Links`].

mod frame;
mod stall;
mod start;

use morf_app::{Backend, Event, PRIMARY_LAYER, Woke};
use morf_desktop::Desktop;
use morf_lua::{Runtime, Screen, SurfaceReserve};
use std::os::fd::BorrowedFd;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::desktop::dispatch_desktop;
use crate::supervisor::LoadPolicy;
use crate::{
    backdrop::*,
    lock::{SupervisorMessage, WorkerCommand, WorkerMessage},
    paint::*,
    services::*,
    surface_events::*,
    surface_layers::*,
    surfaces::*,
    wake_plan::*,
    workers::*,
};

pub(crate) use stall::{advance_without_callbacks, motion_deadline};
pub use start::{FirstFrame, StartOptions};

/// What a host answers to: the supervisor of a live shell. A headless host
/// has none, and its runner plays the supervisor's part itself.
pub struct Links<'a> {
    pub policy: LoadPolicy,
    pub tx: &'a mpsc::Sender<SupervisorMessage>,
    pub commands: &'a mpsc::Receiver<WorkerCommand>,
    pub screen: &'a Screen,
}

/// How a turn ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Turn {
    /// Go round again.
    Again,
    /// The configuration quit, or the output is gone: the loop ends.
    Stop,
    /// The configuration asked for its surface to be made again (a change
    /// to what only a new surface can take); the driver reconnects and hands
    /// the host the new backend ([`Host::replace_backend`]).
    Recreate,
}

/// One output's windows on a backend.
pub struct Host {
    pub name: String,
    /// The window system, seen in morf's pixels (a no-op while the density
    /// is the compositor's), so a change of density reaches it live.
    pub backend: Box<morf_app::Dense>,
    /// The desktop protocols, where the backend is a compositor.
    pub desktop: Option<Desktop>,
    pub state: SurfaceEventState,
    /// What the edge reservers were last built from.
    pub reserve: SurfaceReserve,
    clock: String,
    started: Instant,
    a11y: crate::surface_a11y::A11ySurfaces,
    layout_complaint: Option<Instant>,
    motion_reported: Option<Instant>,
    jit_logged: Option<Instant>,
    /// Whether the last turn handled anything -- an event, a command -- whose
    /// handlers may have left work that only the checks at the top of a turn
    /// pick up (a popup to open, a reload asked for). One more turn, at once.
    follow_up: bool,
    pending_streak: u32,
    /// A node first asked for its `contains_pointer` last turn, whose answer
    /// changed what bindings drew: this turn paints it.
    containment_repaint: bool,
    /// Whether slow stages are reported on stderr.
    report_slow: bool,
}

impl Host {
    /// Waits until something is due: the backend sends something, a service
    /// thread rings `wake`, or the first thing on the clock. Nothing else: an
    /// idle shell sleeps until one of those.
    pub fn wait(
        &mut self,
        runtime: &Runtime,
        wake: Option<BorrowedFd<'_>>,
    ) -> Result<Woke, String> {
        let sleep = Sleep::plan_with(
            runtime,
            std::mem::take(&mut self.follow_up) || self.backend.has_queued_events(),
            motion_deadline(runtime, &*self.backend, &self.state),
            &mut self.pending_streak,
        );
        let slept = Instant::now();
        let woke = self.backend.wait(sleep.timeout(), wake)?;
        log_wake(&self.name, woke, &sleep, slept);
        Ok(woke)
    }

    /// Whether the host has work for a turn at once, without waiting.
    pub fn wants_turn(&self) -> bool {
        self.follow_up || self.backend.has_queued_events()
    }

    /// Whether the last turn left nothing for another at once, for a driver
    /// that does not [`Host::wait`] (headless): takes the follow-up the turn
    /// asked for, as a wait would.
    pub fn settled(&mut self) -> bool {
        !std::mem::take(&mut self.follow_up) && !self.backend.has_queued_events()
    }

    /// One turn: the desktop's news, the clock, services and timers, the
    /// supervisor's commands, the configuration's window and surface changes,
    /// the backend's events, and the paint of whatever changed.
    pub fn turn(
        &mut self,
        runtime: &mut Runtime,
        links: Option<&Links<'_>>,
    ) -> Result<Turn, String> {
        let name = self.name.clone();
        let mut repaint = std::mem::take(&mut self.containment_repaint);
        if let Some(desktop) = self.desktop.as_mut() {
            repaint |= dispatch_desktop(runtime, desktop, self.state.painter.gpu())?;
        }
        self.report_motion(runtime);
        // Before the services, so a callback reading the time reads it as it
        // is now, not as it was when the loop last woke.
        let next_clock = clock_text();
        if next_clock != self.clock {
            self.clock = next_clock;
            repaint |= runtime
                .update_clock(&self.clock)
                .map_err(|error| error.to_string())?;
        }
        self.report_jit(runtime);
        let polling = Instant::now();
        repaint |= runtime.poll_services();
        slow(
            self.report_slow,
            &name,
            "services, timers and callbacks",
            polling,
        );
        let mut recreate_surface = false;
        if let Some(links) = links {
            while let Ok(command) = links.commands.try_recv() {
                self.follow_up = true;
                let started_command = Instant::now();
                let update =
                    handle_worker_command(runtime, Some(links.screen), links.policy, command);
                slow(self.report_slow, &name, "an IPC call", started_command);
                repaint |= update.repaint;
                recreate_surface |= update.recreate_surface;
                if update.reset_input {
                    self.state.input.reset();
                }
                if update.refresh_idle
                    && let Some(desktop) = self.desktop.as_mut()
                {
                    desktop.set_idle_timeouts(&runtime.idle_timeouts());
                }
                if update.reloaded {
                    self.reloaded(runtime)?;
                    // What to do once every output is gone may have changed.
                    links
                        .tx
                        .send(SupervisorMessage::Worker(WorkerMessage::Outputless {
                            output: name.clone(),
                            wanted: runtime.layer_surface_config().outputless,
                        }))
                        .map_err(|_| "output supervisor stopped".to_owned())?;
                }
            }
            if let Some(hard) = runtime.take_reload_request() {
                links
                    .tx
                    .send(SupervisorMessage::Reload { hard })
                    .map_err(|_| "output supervisor stopped".to_owned())?;
            }
            if runtime.quit_requested() {
                // Told once, and then this output stops driving frames. The
                // supervisor takes the others down; returning here rather than
                // waiting for it keeps this thread from painting a shell that
                // is already leaving.
                links
                    .tx
                    .send(SupervisorMessage::Quit)
                    .map_err(|_| "output supervisor stopped".to_owned())?;
                return Ok(Turn::Stop);
            }
            if let Some(enabled) = runtime.take_watch_files_change() {
                links
                    .tx
                    .send(SupervisorMessage::WatchFiles(enabled))
                    .map_err(|_| "output supervisor stopped".to_owned())?;
            }
        }
        if recreate_surface {
            return Ok(Turn::Recreate);
        }
        apply_idle_inhibit(runtime, &mut *self.backend);
        if let Some(desktop) = self.desktop.as_mut() {
            apply_idle_timeouts(runtime, desktop);
        }
        apply_shortcuts_inhibit(runtime, &mut *self.backend);
        if let Some(density) = runtime.take_density_change() {
            // A scale slider: every window is told its new scale and size,
            // and lays itself out again at them.
            self.backend.set_density(density);
            repaint = true;
        }
        if runtime.take_layer_surface_change() {
            self.layer_surface_changed(runtime)?;
            // The mask lives in the same configuration and is re-derived when
            // the surface paints, so the new geometry owes one frame even when
            // the backend has no configure to send back.
            repaint = true;
        }
        if runtime.take_window_surface_change() {
            // The only thing that can move the primary root.
            self.state.primary_root =
                primary_surface_root_keeping(runtime, self.state.primary_root)?;
            repaint |= crate::surfaces::sync_window_surfaces(
                runtime,
                &mut *self.backend,
                &mut self.state.windows,
                &name,
            )?;
        }
        apply_service_requests(runtime, &mut *self.backend, self.desktop.as_mut());
        while let Some(event) = self.backend.next_event() {
            self.follow_up = true;
            repaint |= self.handle_event(runtime, event, links.map(|links| links.tx))?;
        }
        apply_service_requests(runtime, &mut *self.backend, self.desktop.as_mut());
        if let Some(renderer) = self.state.painter.gpu() {
            crate::capture::apply_capture_releases(runtime, renderer);
        }
        crate::surface_actions::apply_window_surface_actions(
            runtime,
            &*self.backend,
            &self.state.windows,
        );
        advance_without_callbacks(runtime, &*self.backend, &mut self.state)?;
        self.frame(runtime, repaint)
    }

    /// Hands the host one event, as the turn does each one the backend
    /// queued. Returns whether it changed what is drawn.
    pub fn handle_event(
        &mut self,
        runtime: &mut Runtime,
        event: Event,
        tx: Option<&mpsc::Sender<SupervisorMessage>>,
    ) -> Result<bool, String> {
        let handling = Instant::now();
        let what = event_kind(&event);
        let handled = handle_surface_event(
            runtime,
            &mut *self.backend,
            self.desktop.as_mut(),
            &mut self.state,
            event,
            tx,
            &self.name,
        );
        slow(self.report_slow, &self.name, what, handling);
        match handled {
            Ok(painted) => Ok(painted),
            // A scene that is mid-change (a view that removed a node whose
            // parent still lists it) fails the layout for this event; the
            // event is dropped and the shell stays up, and the next paint
            // sees whatever the scene has become.
            Err(error) if is_layout_error(&error) => {
                complain_layout(&self.name, &error, &mut self.layout_complaint);
                Ok(true)
            }
            Err(error) => Err(error),
        }
    }

    /// A configuration reloaded in place: node handles and layout revisions
    /// belong to a single runtime, and a new configuration may reuse an old
    /// handle for an unrelated node, or build an entirely different tree (a
    /// theme switch).
    pub fn reloaded(&mut self, runtime: &mut Runtime) -> Result<(), String> {
        self.state.primary_root = primary_surface_root(runtime)?;
        self.state.layout.invalidate_scene();
        self.state.animating_shaders = runtime.shaders_animate();
        if let Some(renderer) = self.state.painter.gpu() {
            crate::surface_run::register_shaders(runtime, renderer)?;
        }
        for surface in self.state.windows.values_mut() {
            surface.layout = None;
        }
        if let Some(desktop) = self.desktop.as_mut() {
            let _ = desktop.reset_gamma(None);
        }
        Ok(())
    }

    /// `morf.surface` changed. Layer shell allows all of this on a mapped
    /// surface, so the shell's own geometry follows without a reconnect; the
    /// configure it provokes resizes the renderer in place.
    fn layer_surface_changed(&mut self, runtime: &mut Runtime) -> Result<(), String> {
        let config = runtime.layer_surface_config();
        self.backend
            .set_layer_geometry(PRIMARY_LAYER, &runtime_bar_config(&config, &self.name)?)?;
        if config.reserve != self.reserve {
            self.reserve = config.reserve;
            open_reserve_layers(&mut *self.backend, &config, &self.name)?;
        }
        apply_primary_opaque(runtime, &*self.backend);
        apply_backdrop(&mut *self.backend, &config, &self.name);
        Ok(())
    }

    /// Now and then, what keeps the loop drawing, when anything does.
    fn report_motion(&mut self, runtime: &Runtime) {
        if wake_log_wanted()
            && runtime.has_motion()
            && self
                .motion_reported
                .is_none_or(|at: Instant| at.elapsed() >= Duration::from_secs(2))
        {
            self.motion_reported = Some(Instant::now());
            for line in runtime.motion_report(8) {
                eprintln!("morf: output {}: moving: {line}", self.name);
            }
        }
    }

    /// `MORF_JIT_LOG`: what the native Lua tier has run, every ten seconds.
    fn report_jit(&mut self, runtime: &Runtime) {
        if jit_log_wanted()
            && self
                .jit_logged
                .is_none_or(|at: Instant| at.elapsed() >= Duration::from_secs(10))
        {
            self.jit_logged = Some(Instant::now());
            if let Some(report) = runtime.jit_report() {
                eprintln!("morf: output {}: jit: {report}", self.name);
            }
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

/// [`crate::surface_run::slow`], when the host reports slow stages: a host
/// that only lays out (a test) has frames far slower than a person notices
/// as a matter of course, and says nothing of them.
pub(crate) fn slow(report: bool, name: &str, what: &str, since: Instant) {
    if report {
        crate::surface_run::slow(name, what, since);
    } else {
        morf_lua::profile::clear();
    }
}
