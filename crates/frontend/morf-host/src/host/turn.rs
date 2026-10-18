//! The host: one output's windows on a backend, and the turn of its loop.
//!
//! The same code runs a shell on a compositor and a configuration under
//! `morf check` and `morf test`: what differs is the backend under it (the
//! Wayland client, or the headless one with its virtual outputs, seat and
//! clock), whether it paints with a GPU or only lays out, and whether a
//! supervisor is on the other end of [`Links`].

mod frame;
mod motion;
mod stall;
mod start;
mod stats;
mod steps;

use stats::FrameAccounting;
use steps::Timed;

use morf_app::{Backend, Event, PRIMARY_LAYER, Woke};
use morf_desktop::Desktop;
use morf_lua::profile::Stage;
use morf_lua::{Runtime, Screen, SurfaceReserve};
use std::os::fd::BorrowedFd;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::host::desktop::dispatch_desktop;
use crate::process::supervisor::LoadPolicy;
use crate::{
    host::events::*,
    host::layers::*,
    host::surfaces::*,
    lock::{SupervisorMessage, WorkerCommand},
    paint::backdrop::*,
    paint::wake_plan::*,
    process::services::*,
    process::workers::*,
};

pub(crate) use stall::{advance_without_callbacks, motion_deadline};
pub use start::{FirstFrame, StartOptions};
pub use stats::{FrameReport, LATE, OVERRUN, OWNERS};

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
    pub(crate) native_contacts: crate::host::provider::handoff::Contacts,
    pub(crate) runtime_generation: u64,
    /// Witnessed the primary surface's configure on this backend generation.
    pub(crate) primary_configured: bool,
    pub name: String,
    /// The window system, seen in morf's pixels (a no-op while the density
    /// is the compositor's), so a change of density reaches it live.
    pub backend: Box<morf_app::Dense>,
    /// The desktop protocols: the compositor's, or a headless stand-in.
    pub desktop: Box<dyn Desktop>,
    pub state: SurfaceEventState,
    /// What the edge reservers were last built from.
    pub reserve: SurfaceReserve,
    clock: String,
    started: Instant,
    a11y: crate::a11y::surface::A11ySurfaces,
    layout_complaint: Option<Instant>,
    frame_accounting: FrameAccounting,
    motion_reported: Option<Instant>,
    jit_logged: Option<Instant>,
    /// Whether the last turn handled anything -- an event, a command -- whose
    /// handlers may have left work that only the checks at the top of a turn
    /// pick up (a popup to open, a reload asked for). One more turn, at once.
    follow_up: bool,
    pending_streak: u32,
    /// Whether a service thread rang the loop since the last turn, and how
    /// often the alarm had rung when last told.
    services_rang: bool,
    alarm_rings: u64,
    /// A node first asked for its `contains_pointer` last turn, whose answer
    /// changed what bindings drew: this turn paints it.
    containment_repaint: bool,
    /// Whether slow stages are reported on stderr.
    report_slow: bool,
    /// The runtime the last reload replaced, and when: dropped on a quiet
    /// turn once the new one has painted, or after [`RETIRED_GRACE`].
    retired: Option<(Runtime, Instant)>,
}

/// How long a replaced runtime waits for a quiet turn to be dropped in.
const RETIRED_GRACE: Duration = Duration::from_secs(2);

impl Host {
    pub(crate) fn request_paint(&mut self) {
        self.follow_up = true;
        self.containment_repaint = true;
    }
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
    /// the backend's events, and the paint of whatever changed -- or, owed
    /// only for frame callbacks, the paint first and the rest after
    /// (`Host::motion_turn`). Charged to the frame accounting however it
    /// ends.
    pub fn turn(
        &mut self,
        runtime: &mut Runtime,
        links: Option<&Links<'_>>,
    ) -> Result<Turn, String> {
        self.frame_accounting.begin_turn();
        runtime.set_host_motion(self.state.turning.is_some());
        let base = morf_lua::profile::stage(Stage::Services);
        let events: Vec<Event> = std::iter::from_fn(|| self.backend.next_event()).collect();
        let commands: Vec<WorkerCommand> =
            links.map_or_else(Vec::new, |links| links.commands.try_iter().collect());
        let turned = match self.motion_only(&events, &commands) {
            true => self.motion_turn(runtime, links, events),
            false => self.ordered_turn(runtime, links, events, commands),
        };
        drop(base);
        let moving = runtime.has_motion() || self.state.animating() || self.state.primary_deferred;
        self.frame_accounting.end_turn(moving);
        turned
    }

    /// A turn in its full order, given the backend's events and the
    /// supervisor's commands queued when it began.
    fn ordered_turn(
        &mut self,
        runtime: &mut Runtime,
        links: Option<&Links<'_>>,
        events: Vec<Event>,
        commands: Vec<WorkerCommand>,
    ) -> Result<Turn, String> {
        let name = self.name.clone();
        let mut repaint = std::mem::take(&mut self.containment_repaint);
        repaint |= dispatch_desktop(runtime, &mut *self.desktop, self.state.painter.gpu())?;
        self.report_motion(runtime);
        // Before the services, so a callback reading the time reads it as it
        // is now, not as it was when the loop last woke.
        repaint |= self.update_clock(runtime)?;
        self.report_jit(runtime);
        repaint |= self.poll_stage(runtime);
        let mut recreate_surface = false;
        if let Some(links) = links {
            let mut queued = commands.into_iter();
            while let Some(command) = queued.next().or_else(|| links.commands.try_recv().ok()) {
                let Some(command) = self.answer_frames_hook(command) else {
                    continue;
                };
                self.follow_up = true;
                if let WorkerCommand::Screens(screens) = &command {
                    repaint |= crate::paint::cover::screens_changed(
                        &mut self.state,
                        &*self.backend,
                        screens,
                        &name,
                    );
                }
                let reload = matches!(command, WorkerCommand::Reload { .. });
                let mut update = {
                    let _timed =
                        Timed::start(self.report_slow, &name, Stage::Handlers, "an IPC call");
                    handle_worker_command(runtime, Some(links.screen), links.policy, command)
                };
                if let Some(old) = update.retired.take() {
                    self.retired = Some((old, Instant::now()));
                }
                // Worked or not, a held frame waits for this reload no longer.
                if let Some(turning) = self.state.turning.as_mut().filter(|_| reload) {
                    turning.rebuilt();
                }
                repaint |= update.repaint;
                recreate_surface |= update.recreate_surface;
                if update.reset_input {
                    self.state.input.reset();
                }
                if update.refresh_idle {
                    refresh_idle_timeouts(runtime, &mut *self.desktop);
                }
                if update.reloaded {
                    self.after_reload(runtime, links)?;
                }
            }
            if let Some(turn) = self.tell_supervisor(runtime, links)? {
                return Ok(turn);
            }
        }
        if recreate_surface {
            return Ok(Turn::Recreate);
        }
        repaint |= self.apply_runtime_changes(runtime)?;
        repaint |= self.handle_events(runtime, events, links, true)?;
        self.finish_turn(runtime, repaint)
    }

    /// Hands the host one event, as the turn does each one the backend
    /// queued. Returns whether it changed what is drawn.
    pub fn handle_event(
        &mut self,
        runtime: &mut Runtime,
        event: Event,
        tx: Option<&mpsc::Sender<SupervisorMessage>>,
    ) -> Result<bool, String> {
        self.native_contacts.observe(&event);
        // A press on the primary surface takes the reload-error band down;
        // one on the band itself goes no further.
        let band = self.state.layout.band.as_ref();
        let dismissed = crate::host::reload_band::dismiss_reload_band(runtime, band, &event);
        if dismissed == Some(true) {
            return Ok(true);
        }
        let handling = Instant::now();
        let what = event_kind(&event);
        let handled = handle_surface_event(
            runtime,
            &mut *self.backend,
            &mut *self.desktop,
            &mut self.state,
            event,
            tx,
            &self.name,
        );
        slow(self.report_slow, &self.name, what, handling);
        match handled {
            Ok(painted) => Ok(painted || dismissed.is_some()),
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
            crate::host::run::register_shaders(runtime, renderer)?;
        }
        for surface in self.state.windows.values_mut() {
            surface.layout = None;
        }
        if self.desktop.capabilities().gamma_control {
            let _ = self.desktop.reset_gamma(None);
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

    /// Now and then, what keeps the loop drawing, when anything does, and
    /// the frame accounting since the last report.
    fn report_motion(&mut self, runtime: &Runtime) {
        if wake_log_wanted()
            && (runtime.has_motion() || self.frame_accounting.overruns() > 0)
            && self
                .motion_reported
                .is_none_or(|at: Instant| at.elapsed() >= Duration::from_secs(2))
        {
            self.motion_reported = Some(Instant::now());
            for line in runtime.motion_report(8) {
                eprintln!("morf: output {}: moving: {line}", self.name);
            }
            eprintln!(
                "{} morf: output {}: {}",
                stamp(),
                self.name,
                self.frame_accounting.take()
            );
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

/// [`crate::host::run::slow`], when the host reports slow stages: a host
/// that only lays out (a test) has frames far slower than a person notices
/// as a matter of course, and says nothing of them.
pub(crate) fn slow(report: bool, name: &str, what: &str, since: Instant) {
    if report {
        crate::host::run::slow(name, what, since);
    } else {
        morf_lua::profile::clear();
    }
}
