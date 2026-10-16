//! A turn owed only for frame callbacks: motion ticks and paints first,
//! and the Lua the turn would otherwise run before the paint -- the
//! desktop's news, the clock, timers, service results and the effect
//! flush after them -- runs after it. What that Lua changes is owed to the
//! next frame. Input, a supervisor's command or a service thread ringing
//! the loop takes the full order instead.

use morf_app::Event;
use morf_lua::Runtime;

use crate::host::desktop::{apply_service_requests, dispatch_desktop};
use crate::lock::WorkerCommand;

use super::steps::is_frame_callback;
use super::{Host, Links, Turn};

impl Host {
    /// A service thread rang the loop's alarm `rings` times since it was
    /// made: news since the last turn takes the full order.
    pub fn rang(&mut self, rings: u64) {
        self.services_rang |= rings != self.alarm_rings;
        self.alarm_rings = rings;
    }

    /// Whether this turn is owed only for frame callbacks: the backend sent
    /// nothing else, the supervisor nothing at all, and no service thread
    /// rang since the last turn.
    pub(super) fn motion_only(&mut self, events: &[Event], commands: &[WorkerCommand]) -> bool {
        let rang = std::mem::take(&mut self.services_rang);
        !rang && commands.is_empty() && !events.is_empty() && events.iter().all(is_frame_callback)
    }

    /// The callbacks ticked and painted, then the services polled; what
    /// they owe is painted next frame, and a turn follows at once for what
    /// only the top of a turn picks up.
    pub(super) fn motion_turn(
        &mut self,
        runtime: &mut Runtime,
        links: Option<&Links<'_>>,
        events: Vec<Event>,
    ) -> Result<Turn, String> {
        self.frame_accounting.motion_turn();
        let mut repaint = std::mem::take(&mut self.containment_repaint);
        self.report_motion(runtime);
        if let Some(links) = links
            && let Some(turn) = self.tell_supervisor(runtime, links)?
        {
            return Ok(turn);
        }
        repaint |= self.apply_runtime_changes(runtime)?;
        repaint |= self.handle_events(runtime, events, links, false)?;
        match self.finish_turn(runtime, repaint)? {
            Turn::Again => {}
            other => return Ok(other),
        }
        let mut owed = dispatch_desktop(runtime, &mut *self.desktop, self.state.painter.gpu())?;
        owed |= self.update_clock(runtime)?;
        self.report_jit(runtime);
        owed |= self.poll_stage(runtime);
        apply_service_requests(runtime, &mut *self.backend, &mut *self.desktop);
        if owed {
            self.request_paint();
        }
        self.follow_up = true;
        Ok(Turn::Again)
    }
}
