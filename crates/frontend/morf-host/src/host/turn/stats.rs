//! Frame-time accounting: how many frames in the window ran past 2.4x
//! the refresh interval, and which stage owned the time. A counter, not
//! a log line: it accumulates on every turn and is read (and reset) by
//! the motion report. The stages are the host-visible owners of a turn:
//! `services/flush` is `Runtime::poll_services` (timers, service results
//! and the reactive effect flush run inside it), `handlers` is the
//! supervisor commands and backend events, and `layout/paint` is the
//! frame itself (layout runs inside the paint).

use std::time::Duration;

#[derive(Clone, Copy, Default)]
struct Stage {
    total: Duration,
    worst: Duration,
}

impl Stage {
    fn record(&mut self, took: Duration) {
        self.total += took;
        self.worst = self.worst.max(took);
    }
}

/// One reporting window of frame accounting.
#[derive(Default)]
pub(crate) struct FrameAccounting {
    frames: u64,
    overruns: u64,
    worst_frame: Duration,
    services: Stage,
    handlers: Stage,
    paint: Stage,
}

impl FrameAccounting {
    /// Records one turn: its stage times, and whether the whole turn
    /// ran past 2.4x the refresh interval.
    pub fn record(
        &mut self,
        frame: Duration,
        refresh: Duration,
        services: Duration,
        handlers: Duration,
        paint: Duration,
    ) {
        self.frames += 1;
        self.worst_frame = self.worst_frame.max(frame);
        if frame > refresh.mul_f64(2.4) {
            self.overruns += 1;
        }
        self.services.record(services);
        self.handlers.record(handlers);
        self.paint.record(paint);
    }

    pub fn overruns(&self) -> u64 {
        self.overruns
    }

    /// The window as one line, and the counters reset for the next one.
    pub fn take_summary(&mut self) -> String {
        let taken = std::mem::take(self);
        let stage = |name: &str, stage: Stage| {
            format!(
                "{name} {:.2}/{:.2}ms",
                stage.total.as_secs_f64() * 1000.0,
                stage.worst.as_secs_f64() * 1000.0
            )
        };
        format!(
            "frames {} ({} over 2.4x refresh, worst {:.2}ms): {} {} {}",
            taken.frames,
            taken.overruns,
            taken.worst_frame.as_secs_f64() * 1000.0,
            stage("services/flush", taken.services),
            stage("handlers", taken.handlers),
            stage("layout/paint", taken.paint),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overruns_count_against_the_refresh_interval() {
        let mut accounting = FrameAccounting::default();
        let refresh = Duration::from_millis(16);
        accounting.record(
            Duration::from_millis(10),
            refresh,
            Duration::from_millis(4),
            Duration::from_millis(1),
            Duration::from_millis(5),
        );
        accounting.record(
            Duration::from_millis(50),
            refresh,
            Duration::from_millis(40),
            Duration::from_millis(2),
            Duration::from_millis(8),
        );
        assert_eq!(accounting.overruns(), 1);
        let summary = accounting.take_summary();
        assert!(
            summary.contains("frames 2 (1 over 2.4x refresh"),
            "{summary}"
        );
        assert!(
            summary.contains("services/flush 44.00/40.00ms"),
            "{summary}"
        );
        // Reset on report: the next window starts empty.
        assert_eq!(accounting.overruns(), 0);
        assert!(accounting.take_summary().contains("frames 0"));
    }
}
