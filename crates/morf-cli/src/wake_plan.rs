//! How long an output's loop sleeps, and what woke it.
//!
//! The loop sleeps in `poll` on the compositor's socket and its alarm (the
//! eventfd every service thread rings when it produces). Neither needs a
//! timeout; what does is time: the earliest timer, caret blink, playing
//! picture or D-Bus timeout the runtime holds, the clock turning over at the
//! finest grain anything reads, and motion ticked by the wall clock while the
//! compositor sends no frame callbacks. With none of those, the loop sleeps
//! until something happens -- an idle shell costs nothing.
//!
//! `MORF_WAKE_LOG=1` prints every wake and its cause, which is how an idle
//! cost in the field is traced to the thing that causes it.

use morf_lua::{ClockPrecision, DeadlineCause, Runtime};
use morf_wayland::Woke;
use std::time::{Duration, Instant};

/// Why the loop set the deadline it set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    /// Something the runtime holds comes due.
    Runtime(DeadlineCause),
    /// The clock turns over at the finest grain a binding reads.
    Clock(ClockPrecision),
    /// Motion ticked by the wall clock, the compositor sending no callbacks.
    Fallback,
    /// The last turn left work for the next; no sleep at all.
    Pending,
}

impl Reason {
    fn name(self) -> &'static str {
        match self {
            Self::Runtime(cause) => cause.name(),
            Self::Clock(ClockPrecision::Seconds) => "clock-seconds",
            Self::Clock(ClockPrecision::Minutes) => "clock-minutes",
            Self::Clock(ClockPrecision::Hours) => "clock-hours",
            Self::Fallback => "fallback",
            Self::Pending => "pending",
        }
    }
}

/// When the loop wakes on its own, and why; `None` sleeps until an event.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Sleep {
    pub(crate) deadline: Option<(Instant, Reason)>,
}

impl Sleep {
    /// The earliest of everything that comes due. `pending` says the last
    /// turn handled events or commands, whose handlers may have left work
    /// behind, which is a turn now rather than a sleep; so is work the
    /// runtime says it holds. `motion` is the caller's own wall-clock tick,
    /// when it keeps one.
    #[cfg(test)]
    pub(crate) fn plan(runtime: &Runtime, pending: bool, motion: Option<Instant>) -> Self {
        Self::plan_with(runtime, pending, motion, &mut 0)
    }

    /// As [`Self::plan`], counting in `streak` the turns taken at once for
    /// work the runtime held. Work a turn cannot clear -- a view whose model
    /// changed and whose reconcile keeps failing -- must not spin the loop:
    /// past a few turns in a row it waits for the next wake like the rest.
    pub(crate) fn plan_with(
        runtime: &Runtime,
        pending: bool,
        motion: Option<Instant>,
        streak: &mut u32,
    ) -> Self {
        const MOST_TURNS_IN_A_ROW: u32 = 4;
        let now = Instant::now();
        let held = runtime.has_pending_work() && *streak < MOST_TURNS_IN_A_ROW;
        *streak = if held { *streak + 1 } else { 0 };
        if pending || held {
            return Self {
                deadline: Some((now, Reason::Pending)),
            };
        }
        let clock = runtime
            .clock_precision()
            .map(|precision| (now + precision.until_next(), Reason::Clock(precision)));
        let deadline = [
            runtime
                .next_deadline()
                .map(|(at, cause)| (at, Reason::Runtime(cause))),
            clock,
            motion.map(|at| (at, Reason::Fallback)),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|(at, _)| *at);
        Self { deadline }
    }

    /// How long to sleep from now; `None` is until an event.
    pub(crate) fn timeout(&self) -> Option<Duration> {
        self.deadline
            .map(|(at, _)| at.saturating_duration_since(Instant::now()))
    }
}

/// Whether `MORF_WAKE_LOG` asks for every wake to be printed.
pub(crate) fn wake_log_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| {
        std::env::var_os("MORF_WAKE_LOG").is_some_and(|value| !value.is_empty() && value != "0")
    })
}

/// What ended one sleep, in words.
pub(crate) fn wake_cause(woke: Woke, sleep: &Sleep) -> String {
    match woke {
        Woke::Queued => "queued events".to_owned(),
        Woke::Compositor => "compositor".to_owned(),
        Woke::Alarm => "wake fd".to_owned(),
        Woke::Timeout => match sleep.deadline {
            Some((_, reason)) => format!("deadline: {}", reason.name()),
            None => "deadline".to_owned(),
        },
    }
}

/// Prints one wake, when `MORF_WAKE_LOG` asks: what ended the sleep, how
/// long it lasted, and what the loop had planned to wake for.
pub(crate) fn log_wake(output: &str, woke: Woke, sleep: &Sleep, slept: Instant) {
    if !wake_log_wanted() {
        return;
    }
    let planned = match sleep.deadline {
        Some((at, reason)) => format!(
            "{} in {:.1} ms",
            reason.name(),
            at.saturating_duration_since(slept).as_secs_f64() * 1000.0
        ),
        None => "nothing".to_owned(),
    };
    eprintln!(
        "{} morf: output {output}: wake: {} after {:.1} ms (planned: {planned})",
        stamp(),
        wake_cause(woke, sleep),
        slept.elapsed().as_secs_f64() * 1000.0,
    );
}

pub(crate) use morf_lua::profile::stamp;
