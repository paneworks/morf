//! Frame-time accounting: for each paint of the shell's own surface, where
//! the time since the frame before it went, stage by stage
//! ([`morf_lua::profile::Stage`], and `waiting` for time no stage held: the
//! loop asleep, the compositor, a runner between turns), and how many
//! frames in the window ran past 2.4x the refresh interval, each such
//! overrun counted against the stage that held most of it.
//!
//! A frame runs from the paint before it while motion carries on between
//! the two, and from the start of its own turn otherwise, so a shell that
//! sat still for a minute does not count the minute. The refresh interval
//! is the median of the last frames' callback intervals, which the one
//! long interval after a stall does not move. Always on; read and reset by
//! the motion report (`MORF_WAKE_LOG`), the `@frames` test hook and
//! `morf test`.

use std::fmt;
use std::time::{Duration, Instant};

use morf_lua::IpcValue;
use morf_lua::profile::{StageTimes, take_stages};

use super::Host;
use crate::lock::WorkerCommand;

/// How many refresh intervals a frame runs before it is an overrun.
pub const OVERRUN: f64 = 2.4;
/// How many before it is late: it missed at least one refresh.
pub const LATE: f64 = 1.5;
/// What a frame's time is charged to: each stage, then none of them.
pub const OWNERS: [&str; 7] = [
    "services", "handlers", "flush", "motion", "layout", "paint", "waiting",
];
const WAITING: usize = OWNERS.len() - 1;
/// How many frames' callback intervals the refresh interval is the median of.
const REFRESHES: usize = 15;
/// The hidden verb (with `MORF_TEST_HOOKS`) that answers this output's
/// report and starts a new window.
const FRAMES_HOOK: &str = "@frames";

/// One window of frame accounting.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameReport {
    pub turns: u64,
    /// Turns owed only for frame callbacks, painted before their services.
    pub motion_turns: u64,
    pub frames: u64,
    pub late: u64,
    pub overruns: u64,
    pub worst: Duration,
    pub total: Duration,
    /// The refresh interval at the last frame.
    pub refresh: Duration,
    /// Time charged to each of [`OWNERS`] over the window's frames.
    pub stages: [Duration; OWNERS.len()],
    /// How many overruns each of [`OWNERS`] held the most of.
    pub owners: [u64; OWNERS.len()],
}

impl FrameReport {
    fn index(owner: &str) -> Option<usize> {
        OWNERS.iter().position(|name| *name == owner)
    }

    /// Time charged to `owner`, one of [`OWNERS`].
    pub fn stage(&self, owner: &str) -> Duration {
        Self::index(owner).map_or(Duration::ZERO, |index| self.stages[index])
    }

    /// How many overruns `owner` held the most of.
    pub fn owned(&self, owner: &str) -> u64 {
        Self::index(owner).map_or(0, |index| self.owners[index])
    }

    pub fn mean(&self) -> Duration {
        match u32::try_from(self.frames) {
            Ok(frames) if frames > 0 => self.total / frames,
            _ => Duration::ZERO,
        }
    }
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

impl fmt::Display for FrameReport {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let owners = OWNERS
            .iter()
            .zip(self.owners)
            .filter(|(_, count)| *count > 0)
            .map(|(name, count)| format!("{name} {count}"))
            .collect::<Vec<_>>();
        let stages = OWNERS
            .iter()
            .zip(self.stages)
            .map(|(name, took)| format!("{name} {:.1}", ms(took)))
            .collect::<Vec<_>>();
        write!(
            out,
            "frames {} (late {}, over {OVERRUN}x refresh {}{}{}), mean {:.1} worst {:.1} ms \
             at {:.1} ms refresh; motion-only turns {} of {}; {} ms",
            self.frames,
            self.late,
            self.overruns,
            if owners.is_empty() { "" } else { ": " },
            owners.join(" "),
            ms(self.mean()),
            ms(self.worst),
            ms(self.refresh),
            self.motion_turns,
            self.turns,
            stages.join(" "),
        )
    }
}

/// The accounting a host keeps: the window so far, and the frame under way.
#[derive(Default)]
pub(crate) struct FrameAccounting {
    report: FrameReport,
    since: Option<Instant>,
    pending: StageTimes,
    /// The scene's paint revision at the last frame.
    shown: Option<u64>,
    /// The last frames' callback intervals.
    intervals: std::collections::VecDeque<Duration>,
}

impl FrameAccounting {
    /// A turn begins: what the clock was charged between turns is no
    /// frame's, and a frame with none under way begins here.
    pub fn begin_turn(&mut self) {
        let _ = take_stages();
        self.report.turns += 1;
        if self.since.is_none() {
            self.since = Some(Instant::now());
            self.pending = StageTimes::default();
        }
    }

    pub fn motion_turn(&mut self) {
        self.report.motion_turns += 1;
    }

    /// The median of the last frames' callback intervals, `interval` the
    /// latest of them.
    fn refresh(&mut self, interval: Duration) -> Duration {
        if self.intervals.len() == REFRESHES {
            self.intervals.pop_front();
        }
        self.intervals.push_back(interval);
        let mut sorted = self.intervals.iter().copied().collect::<Vec<_>>();
        sorted.sort_unstable();
        sorted[sorted.len() / 2]
    }

    /// The shell's own surface was painted at the scene's paint revision
    /// `shown`, `interval` after the callback before: the frame under way
    /// ends, unless the paint showed what the last frame did (`changing`
    /// says it cannot have: a shader moves).
    pub fn painted(&mut self, interval: Duration, shown: u64, changing: bool) {
        if !changing && self.shown == Some(shown) {
            return;
        }
        self.shown = Some(shown);
        let refresh = self.refresh(interval);
        let now = Instant::now();
        let mut charged = [Duration::ZERO; OWNERS.len()];
        for ((owner, held), took) in charged.iter_mut().zip(self.pending).zip(take_stages()) {
            *owner = held + took;
        }
        let frame = self
            .since
            .map_or(Duration::ZERO, |at| now.saturating_duration_since(at));
        charged[WAITING] = frame.saturating_sub(charged.iter().sum());
        let report = &mut self.report;
        report.frames += 1;
        report.total += frame;
        report.worst = report.worst.max(frame);
        report.refresh = refresh;
        for (sum, took) in report.stages.iter_mut().zip(charged) {
            *sum += took;
        }
        if frame > refresh.mul_f64(LATE) {
            report.late += 1;
        }
        if frame > refresh.mul_f64(OVERRUN) {
            report.overruns += 1;
            let mut owner = WAITING;
            for (index, took) in charged.iter().enumerate() {
                if *took > charged[owner] {
                    owner = index;
                }
            }
            report.owners[owner] += 1;
        }
        self.since = Some(now);
        self.pending = StageTimes::default();
    }

    /// A turn ends, however it ends: its time is the next frame's while
    /// motion carries on, and nobody's once the shell is still.
    pub fn end_turn(&mut self, moving: bool) {
        for (held, took) in self.pending.iter_mut().zip(take_stages()) {
            *held += took;
        }
        if !moving {
            self.since = None;
            self.pending = StageTimes::default();
        }
    }

    pub fn overruns(&self) -> u64 {
        self.report.overruns
    }

    /// The window so far, and a new one begun.
    pub fn take(&mut self) -> FrameReport {
        std::mem::take(&mut self.report)
    }
}

impl Host {
    /// This output's frame report since the last one, and a new window.
    pub fn take_frame_report(&mut self) -> FrameReport {
        self.frame_accounting.take()
    }

    /// Answers `@frames` when test hooks are on; any other command is
    /// handed back to be carried out.
    pub(super) fn answer_frames_hook(&mut self, command: WorkerCommand) -> Option<WorkerCommand> {
        match command {
            WorkerCommand::Call { target, reply, .. }
                if target == FRAMES_HOOK && std::env::var_os("MORF_TEST_HOOKS").is_some() =>
            {
                let report = self.frame_accounting.take();
                let line = format!("{}: {report}", self.name);
                let _ = reply.send(Ok(vec![IpcValue::String(line)]));
                None
            }
            other => Some(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use morf_lua::profile::{Stage, stage};

    fn spend(ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }

    const REFRESH: Duration = Duration::from_millis(10);

    #[test]
    fn an_overrun_is_counted_against_the_stage_that_held_it() {
        let mut accounting = FrameAccounting::default();
        accounting.begin_turn();
        {
            let _paint = stage(Stage::Paint);
            spend(2);
        }
        accounting.painted(REFRESH, 1, false);
        accounting.end_turn(true);
        accounting.begin_turn();
        {
            let _handlers = stage(Stage::Handlers);
            spend(40);
        }
        accounting.end_turn(true);
        accounting.begin_turn();
        {
            let _paint = stage(Stage::Paint);
            spend(2);
        }
        accounting.painted(REFRESH, 2, false);
        accounting.end_turn(false);
        let report = accounting.take();
        assert_eq!((report.frames, report.turns), (2, 3), "{report}");
        assert_eq!(report.overruns, 1, "{report}");
        assert_eq!(report.owned("handlers"), 1, "{report}");
        assert!(report.stage("handlers") >= Duration::from_millis(40));
        assert!(report.stage("paint") >= Duration::from_millis(4));
        assert!(report.worst >= Duration::from_millis(42));
        assert_eq!(accounting.take(), FrameReport::default());
    }

    #[test]
    fn a_still_shell_starts_its_next_frame_with_its_next_turn() {
        let mut accounting = FrameAccounting::default();
        accounting.begin_turn();
        accounting.painted(REFRESH, 1, false);
        accounting.end_turn(false);
        spend(40);
        accounting.begin_turn();
        accounting.painted(REFRESH, 2, false);
        accounting.end_turn(false);
        let report = accounting.take();
        assert_eq!((report.frames, report.overruns), (2, 0), "{report}");
        assert!(report.worst < Duration::from_millis(20), "{report}");
    }

    #[test]
    fn the_long_callback_after_a_stall_leaves_the_refresh_where_it_was() {
        let mut accounting = FrameAccounting::default();
        accounting.begin_turn();
        for shown in 1..=5 {
            accounting.painted(REFRESH, shown, false);
        }
        accounting.end_turn(true);
        spend(40);
        accounting.begin_turn();
        accounting.painted(REFRESH * 4, 6, false);
        accounting.end_turn(true);
        let report = accounting.take();
        assert_eq!(report.overruns, 1, "{report}");
        assert_eq!(report.refresh, REFRESH, "{report}");
    }

    #[test]
    fn a_paint_of_the_picture_already_shown_is_no_frame() {
        let mut accounting = FrameAccounting::default();
        accounting.begin_turn();
        accounting.painted(REFRESH, 1, false);
        accounting.painted(REFRESH, 1, false);
        accounting.painted(REFRESH, 1, true);
        accounting.end_turn(false);
        assert_eq!(accounting.take().frames, 2);
    }

    #[test]
    fn time_no_stage_held_is_waiting() {
        let mut accounting = FrameAccounting::default();
        accounting.begin_turn();
        accounting.painted(REFRESH, 1, false);
        accounting.end_turn(true);
        spend(40);
        accounting.begin_turn();
        accounting.painted(REFRESH, 2, false);
        accounting.end_turn(true);
        let report = accounting.take();
        assert_eq!(report.owned("waiting"), 1, "{report}");
        assert!(
            report.stage("waiting") >= Duration::from_millis(40),
            "{report}"
        );
        assert!(
            report
                .to_string()
                .contains("over 2.4x refresh 1: waiting 1"),
            "{report}"
        );
    }
}
