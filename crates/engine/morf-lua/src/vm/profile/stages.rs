//! Where a turn's time goes, by stage: always on, unlike the spans, and
//! keyed by a handful of stages rather than by labels.
//!
//! The engine marks its own stages where the spans already mark their work
//! (a handler, the flush after it, the animation tick) and the host marks
//! the rest of a turn (services, layout, paint). A stage entered inside
//! another charges its own time, the outer one only what is left, so a
//! timer whose flush ran forty bindings is handler time for the timer's
//! own Lua and flush time for the bindings. Time outside every stage is
//! charged to none. One clock per thread, as there is one runtime per
//! output thread.

use std::cell::RefCell;
use std::time::{Duration, Instant};

/// One stage of a turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    /// The engine's and the host's own bookkeeping: services polled,
    /// timers collected, the desktop's news, surfaces kept in step.
    Services,
    /// Lua answering something: a timer, a click, an IPC verb, a callback.
    Handlers,
    /// The reactive flush: bindings and effects re-run after a write.
    Flush,
    /// Animations ticked.
    Motion,
    /// Layout of a surface.
    Layout,
    /// Everything else a paint does: the draw, the input region, the
    /// submission.
    Paint,
}

impl Stage {
    /// Every stage, in the order [`StageTimes`] holds them.
    pub const ALL: [Stage; 6] = [
        Stage::Services,
        Stage::Handlers,
        Stage::Flush,
        Stage::Motion,
        Stage::Layout,
        Stage::Paint,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Stage::Services => "services",
            Stage::Handlers => "handlers",
            Stage::Flush => "flush",
            Stage::Motion => "motion",
            Stage::Layout => "layout",
            Stage::Paint => "paint",
        }
    }
}

/// Time charged to each stage, in [`Stage::ALL`] order.
pub type StageTimes = [Duration; Stage::ALL.len()];

#[derive(Default)]
struct StageClock {
    open: Vec<Stage>,
    mark: Option<Instant>,
    times: StageTimes,
}

impl StageClock {
    /// Charges the time since the last mark to the stage open now.
    fn charge(&mut self, now: Instant) {
        if let (Some(&open), Some(mark)) = (self.open.last(), self.mark) {
            self.times[open as usize] += now.saturating_duration_since(mark);
        }
        self.mark = Some(now);
    }
}

thread_local! {
    static CLOCK: RefCell<StageClock> = RefCell::new(StageClock::default());
}

/// An open stage; its time is charged until it is dropped.
#[must_use = "a stage is charged until it is dropped"]
pub struct StageGuard {
    _not_send: std::marker::PhantomData<*const ()>,
}

/// Opens `stage` on this thread's clock.
pub fn stage(stage: Stage) -> StageGuard {
    CLOCK.with(|clock| {
        let mut clock = clock.borrow_mut();
        clock.charge(Instant::now());
        clock.open.push(stage);
    });
    StageGuard {
        _not_send: std::marker::PhantomData,
    }
}

impl Drop for StageGuard {
    fn drop(&mut self) {
        CLOCK.with(|clock| {
            let mut clock = clock.borrow_mut();
            clock.charge(Instant::now());
            clock.open.pop();
        });
    }
}

/// What each stage was charged since the last take, the stages still open
/// charged up to now, and clears it.
pub fn take_stages() -> StageTimes {
    CLOCK.with(|clock| {
        let mut clock = clock.borrow_mut();
        clock.charge(Instant::now());
        std::mem::take(&mut clock.times)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spend(ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }

    fn ms(times: &StageTimes, stage: Stage) -> u128 {
        times[stage as usize].as_millis()
    }

    #[test]
    fn a_nested_stage_charges_its_own_time_and_the_outer_the_rest() {
        let _ = take_stages();
        {
            let _services = stage(Stage::Services);
            spend(2);
            {
                let _handler = stage(Stage::Handlers);
                spend(40);
                let _flush = stage(Stage::Flush);
                spend(40);
            }
            spend(2);
        }
        let times = take_stages();
        assert!(ms(&times, Stage::Handlers) >= 40, "{times:?}");
        assert!(ms(&times, Stage::Flush) >= 40, "{times:?}");
        assert!(ms(&times, Stage::Services) >= 4, "{times:?}");
        // Charged with an inner stage's time, these would pass 80.
        assert!(ms(&times, Stage::Handlers) < 80, "{times:?}");
        assert!(ms(&times, Stage::Services) < 40, "{times:?}");
        assert_eq!(times[Stage::Paint as usize], Duration::ZERO);
    }

    #[test]
    fn time_outside_every_stage_is_charged_to_none_and_a_take_clears() {
        let _ = take_stages();
        spend(30);
        {
            let _paint = stage(Stage::Paint);
            spend(5);
        }
        spend(30);
        let times = take_stages();
        let total: Duration = times.iter().sum();
        assert!(ms(&times, Stage::Paint) >= 5, "{times:?}");
        assert!(total < Duration::from_millis(30), "{times:?}");
        assert_eq!(take_stages(), StageTimes::default());
    }

    #[test]
    fn a_take_inside_an_open_stage_charges_it_so_far() {
        let _ = take_stages();
        let _layout = stage(Stage::Layout);
        spend(30);
        assert!(ms(&take_stages(), Stage::Layout) >= 30);
        spend(2);
        let again = take_stages();
        assert!(ms(&again, Stage::Layout) >= 2 && ms(&again, Stage::Layout) < 30);
    }
}
