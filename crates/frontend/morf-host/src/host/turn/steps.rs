//! The steps a turn is made of, whichever order it takes them in.

use morf_app::{Backend, Event, PRIMARY_LAYER};
use morf_lua::Runtime;
use morf_lua::profile::{Stage, StageGuard, stage};
use std::time::Instant;

use crate::host::desktop::apply_service_requests;
use crate::lock::{SupervisorMessage, WorkerMessage};
use crate::{host::surfaces::*, paint::*, process::services::*};

use super::{Host, Links, Turn, advance_without_callbacks, slow};

/// One stage of a turn, timed twice over: charged to its stage in the frame
/// accounting, and said by [`slow`] when it held the output.
pub(crate) struct Timed<'a> {
    report: bool,
    name: &'a str,
    what: &'static str,
    started: Instant,
    _stage: StageGuard,
}

impl<'a> Timed<'a> {
    pub(crate) fn start(report: bool, name: &'a str, charged: Stage, what: &'static str) -> Self {
        Self {
            report,
            name,
            what,
            started: Instant::now(),
            _stage: stage(charged),
        }
    }
}

impl Drop for Timed<'_> {
    fn drop(&mut self) {
        slow(self.report, self.name, self.what, self.started);
    }
}

/// Whether an event is a frame callback, for any of the shell's surfaces.
pub(super) fn is_frame_callback(event: &Event) -> bool {
    matches!(
        event,
        Event::Frame { .. } | Event::PopupFrame { .. } | Event::ToplevelFrame { .. }
    )
}

/// Whether the person did something: pointed, scrolled, touched, typed or
/// dragged.
pub(super) fn is_input(event: &Event) -> bool {
    matches!(
        event,
        Event::PointerMotion { .. }
            | Event::PointerButton { .. }
            | Event::PointerAxis { .. }
            | Event::TouchDown { .. }
            | Event::TouchMotion { .. }
            | Event::TouchUp { .. }
            | Event::Key { .. }
            | Event::DragMotion { .. }
    )
}

impl Host {
    /// `morf.time.clock`, when the second turned over since the last turn.
    pub(super) fn update_clock(&mut self, runtime: &mut Runtime) -> Result<bool, String> {
        let next_clock = clock_text();
        if next_clock == self.clock {
            return Ok(false);
        }
        self.clock = next_clock;
        runtime
            .update_clock(&self.clock)
            .map_err(|error| error.to_string())
    }

    /// One services poll: timers, service results and the effect flush
    /// after their callbacks.
    pub(super) fn poll_stage(&self, runtime: &mut Runtime) -> bool {
        let _timed = Timed::start(
            self.report_slow,
            &self.name,
            Stage::Services,
            "services, timers and callbacks",
        );
        runtime.poll_services()
    }

    /// Tells the supervisor what the configuration asked of it: a reload,
    /// to quit (and then this output stops driving frames), files watched.
    pub(super) fn tell_supervisor(
        &mut self,
        runtime: &mut Runtime,
        links: &Links<'_>,
    ) -> Result<Option<Turn>, String> {
        let stopped = |_| "output supervisor stopped".to_owned();
        if let Some(asked) = runtime.take_reload_request() {
            if let Some(turning) = &mut self.state.turning {
                turning.rebuilding();
            }
            let output = asked.output.then(|| self.name.clone());
            let hard = asked.hard;
            links
                .tx
                .send(SupervisorMessage::Reload { hard, output })
                .map_err(stopped)?;
        }
        if runtime.quit_requested() {
            // Told once; the supervisor takes the other outputs down, and
            // this thread paints nothing more of a shell that is leaving.
            links.tx.send(SupervisorMessage::Quit).map_err(stopped)?;
            return Ok(Some(Turn::Stop));
        }
        Ok(None)
    }

    /// A reload the supervisor carried out: what the old configuration
    /// left on the outputs goes, and the supervisor hears what to do once
    /// every output is gone.
    pub(super) fn after_reload(
        &mut self,
        runtime: &mut Runtime,
        links: &Links<'_>,
    ) -> Result<(), String> {
        self.runtime_generation = self
            .runtime_generation
            .checked_add(1)
            .ok_or("native controller runtime generation exhausted")?;
        self.reloaded(runtime)?;
        links
            .tx
            .send(SupervisorMessage::Worker(WorkerMessage::Outputless {
                output: self.name.clone(),
                wanted: runtime.layer_surface_config().outputless,
            }))
            .map_err(|_| "output supervisor stopped".to_owned())
    }

    /// What the configuration changed that the window system has to hear:
    /// its inhibitors, its density, its own surface and its windows.
    /// Returns whether that owes a repaint.
    pub(super) fn apply_runtime_changes(&mut self, runtime: &mut Runtime) -> Result<bool, String> {
        let mut repaint = false;
        apply_idle_inhibit(runtime, &mut *self.backend);
        apply_idle_timeouts(runtime, &mut *self.desktop);
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
            repaint |= crate::host::surfaces::sync_window_surfaces(
                runtime,
                &mut *self.backend,
                &mut self.state.windows,
                &self.name,
            )?;
        }
        apply_service_requests(runtime, &mut *self.backend, &mut *self.desktop);
        Ok(repaint)
    }

    /// Hands the host `events`, and with `more` whatever the backend
    /// queued since; frame callbacks are charged to motion, the rest to
    /// handlers. Returns whether they changed what is drawn.
    pub(super) fn handle_events(
        &mut self,
        runtime: &mut Runtime,
        events: Vec<Event>,
        links: Option<&Links<'_>>,
        more: bool,
    ) -> Result<bool, String> {
        let tx = links.map(|links| links.tx);
        let mut repaint = false;
        let mut events = events.into_iter();
        while let Some(event) = events
            .next()
            .or_else(|| more.then(|| self.backend.next_event()).flatten())
        {
            if matches!(&event, Event::Configure { id, .. } if *id == PRIMARY_LAYER) {
                self.primary_configured = true;
            }
            if is_input(&event) {
                self.last_input = Some(std::time::Instant::now());
            }
            self.follow_up = true;
            let charged = match is_frame_callback(&event) {
                true => Stage::Motion,
                false => Stage::Handlers,
            };
            let _stage = stage(charged);
            repaint |= self.handle_event(runtime, event, tx)?;
        }
        Ok(repaint)
    }

    /// What every turn ends with: the configuration's requests to the
    /// compositor and its windows, motion no callback ticks, and the paint.
    pub(super) fn finish_turn(
        &mut self,
        runtime: &mut Runtime,
        repaint: bool,
    ) -> Result<Turn, String> {
        apply_service_requests(runtime, &mut *self.backend, &mut *self.desktop);
        if let Some(renderer) = self.state.painter.gpu() {
            crate::capture::apply_capture_releases(runtime, renderer);
        }
        crate::host::actions::apply_window_surface_actions(
            runtime,
            &*self.backend,
            &self.state.windows,
        );
        advance_without_callbacks(runtime, &*self.backend, &mut self.state)?;
        self.frame(runtime, repaint)
    }
}
