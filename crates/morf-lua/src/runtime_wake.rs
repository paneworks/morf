//! When the shell's loop has to wake on its own, and why.
//!
//! Everything a thread produces -- a line from a child, a signal off the bus,
//! a decoded image -- rings the loop's alarm when it lands, so the loop needs
//! no timeout for any of that. What is left is time itself: a timer coming
//! due, a caret about to blink, a moving picture's next frame, a D-Bus call
//! that stops being waited for, the clock turning over. This is where the
//! runtime says which of those comes first, so an idle shell sleeps exactly
//! until then and not a moment sooner.

use std::time::{Duration, Instant};

use crate::{Error, IpcValue, reactive_bindings::flush_reactive, types::Runtime};

/// How fine a clock a configuration reads.
///
/// Ordered coarse to fine, so the finest in use is the greatest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClockPrecision {
    Hours,
    Minutes,
    Seconds,
}

impl ClockPrecision {
    /// The precision a `morf.system_clock` was made with; anything else is
    /// seconds, the finest there is, so nothing is ever shown stale.
    pub(crate) fn parse(name: &str) -> Self {
        match name {
            "hours" => Self::Hours,
            "minutes" => Self::Minutes,
            _ => Self::Seconds,
        }
    }

    /// The finest unit a strftime format shows, which is how often text made
    /// from it can change. A conversion this does not know counts as
    /// seconds.
    pub fn of_format(format: &str) -> Self {
        let mut finest = Self::Hours;
        let mut chars = format.chars();
        while let Some(char) = chars.next() {
            if char != '%' {
                continue;
            }
            // Flags, a width, a precision and a colon may come between the
            // percent sign and the conversion: `%-d`, `%_3H`, `%.3f`, `%:z`.
            let conversion = chars
                .by_ref()
                .find(|char| !matches!(char, '-' | '_' | '0'..='9' | '^' | '#' | '.' | ':'));
            let unit = match conversion {
                None => break,
                Some('H' | 'I' | 'k' | 'l' | 'p' | 'P') => Self::Hours,
                Some('M' | 'R') => Self::Minutes,
                Some(
                    'Y' | 'y' | 'C' | 'G' | 'g' | 'm' | 'b' | 'B' | 'h' | 'd' | 'e' | 'j' | 'a'
                    | 'A' | 'u' | 'w' | 'U' | 'W' | 'V' | 'D' | 'F' | 'x' | 'n' | 't' | '%' | 'z'
                    | 'Z' | 'Q',
                ) => Self::Hours,
                Some(_) => Self::Seconds,
            };
            finest = finest.max(unit);
        }
        finest
    }

    /// How long until the local clock next turns over at this grain.
    pub fn until_next(self) -> Duration {
        let now = jiff::Zoned::now();
        let into_second = Duration::from_nanos(now.subsec_nanosecond().max(0) as u64);
        let seconds_left = match self {
            Self::Seconds => 1,
            Self::Minutes => 60 - u64::from(now.second().clamp(0, 59) as u8),
            Self::Hours => {
                (59 - u64::from(now.minute().clamp(0, 59) as u8)) * 60
                    + (60 - u64::from(now.second().clamp(0, 59) as u8))
            }
        };
        // A hair past the boundary, so the wake reads the new time rather
        // than the last instant of the old one.
        (Duration::from_secs(seconds_left) + Duration::from_millis(2)).saturating_sub(into_second)
    }
}

/// Why the loop wakes on its own, for `MORF_WAKE_LOG`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeadlineCause {
    /// A `morf.timer` or a running `ui.Timer`.
    Timer,
    /// The focused field's caret turns on or off.
    Caret,
    /// A playing picture's next frame.
    Image,
    /// A D-Bus call made with a bound stops being waited for.
    DbusTimeout,
    /// A terminal's synchronized update has been held as long as it may be.
    Terminal,
    /// A tray host asks a watcher that did not answer again.
    TrayRetry,
    /// A `Loader` with `preload` has an item to build ahead of time.
    Preload,
}

impl DeadlineCause {
    pub fn name(self) -> &'static str {
        match self {
            Self::Timer => "timer",
            Self::Preload => "preload",
            Self::Caret => "caret",
            Self::Image => "image",
            Self::DbusTimeout => "dbus-timeout",
            Self::Terminal => "terminal",
            Self::TrayRetry => "tray-retry",
        }
    }
}

impl Runtime {
    /// Updates `morf.clock` ("HH:MM:SS"), and `morf.minute_clock` and
    /// `morf.hour_clock` with it, and recomputes what reads them.
    ///
    /// Only the grains that turned over are written, so a binding on the
    /// minute clock runs once a minute however often this is called. Returns
    /// whether the scene actually changed: a repaint of a shell that shows
    /// no time is pure cost.
    pub fn update_clock(&mut self, value: impl Into<String>) -> Result<bool, Error> {
        let revision_before = self.reactive.borrow().scene_revision;
        let value: String = value.into();
        // "HH:MM:SS" carries the coarser grains in its prefix; a value in
        // any other shape is written as it is and nothing is derived.
        let derived = value
            .get(..5)
            .filter(|_| value.len() == 8 && value.as_bytes()[2] == b':')
            .map(|minutes| (minutes.to_owned(), value[..2].to_owned()));
        let mut changed = false;
        {
            let mut state = self.reactive.borrow_mut();
            let mut writes = vec![(state.clock, value)];
            if let Some((minutes, hours)) = derived {
                writes.push((state.clock_minutes, minutes));
                writes.push((state.clock_hours, hours));
            }
            for (signal, text) in writes {
                let text = IpcValue::String(text);
                if state.values.get(&signal) == Some(&text) {
                    continue;
                }
                state
                    .graph
                    .as_mut()
                    .ok_or_else(|| Error::Runtime("reactive graph is already running".to_owned()))?
                    .write(signal, text.clone())
                    .map_err(|error| Error::Runtime(error.to_string()))?;
                state.values.insert(signal, text);
                changed = true;
            }
        }
        if changed {
            self.lua
                .enter(|ctx| flush_reactive(&self.reactive, ctx, self.limits))
                .map_err(Error::Runtime)?;
        }
        Ok(self.reactive.borrow().scene_revision != revision_before)
    }

    /// The finest clock anything currently reads, or nothing when no binding
    /// shows the time: the grain the loop has to wake at for the clock.
    pub fn clock_precision(&self) -> Option<ClockPrecision> {
        let state = self.reactive.borrow();
        let graph = state.graph.as_ref()?;
        [
            (state.clock, ClockPrecision::Seconds),
            (state.clock_minutes, ClockPrecision::Minutes),
            (state.clock_hours, ClockPrecision::Hours),
        ]
        .into_iter()
        .find(|(signal, _)| graph.has_subscribers(*signal))
        .map(|(_, precision)| precision)
    }

    /// The earliest moment something in this runtime comes due on the wall
    /// clock, and what it is; nothing when only an event can bring work.
    ///
    /// The clock is not among them: [`Runtime::clock_precision`] says what
    /// grain it is read at, and the loop owns the wall-clock text.
    pub fn next_deadline(&self) -> Option<(Instant, DeadlineCause)> {
        let state = self.reactive.borrow();
        let timers = state
            .timers
            .iter()
            .filter_map(|timer| timer.timer.wall_deadline())
            .min()
            .map(|at| (at, DeadlineCause::Timer));
        let caret = crate::text_inputs::next_blink(&state).map(|at| (at, DeadlineCause::Caret));
        let image = state.images.due().map(|at| (at, DeadlineCause::Image));
        let dbus = state
            .dbus_replies
            .iter()
            .filter_map(|entry| entry.reply.deadline())
            .min()
            .map(|at| (at, DeadlineCause::DbusTimeout));
        let terminal = state
            .terminals
            .next_deadline()
            .map(|at| (at, DeadlineCause::Terminal));
        let tray = state
            .status_notifiers
            .iter()
            .filter_map(|subscription| subscription.host.next_deadline())
            .min()
            .map(|at| (at, DeadlineCause::TrayRetry));
        // Due at once while the scene is still; moving, only once it has
        // waited as long as a preload waits for anything.
        let still = !state.scene.has_motion();
        let preload = state
            .preload_pending
            .values()
            .min()
            .map(|since| {
                if still {
                    *since
                } else {
                    *since + crate::runtime_services::PRELOAD_PATIENCE
                }
            })
            .map(|at| (at, DeadlineCause::Preload));
        [timers, caret, image, dbus, terminal, tray, preload]
            .into_iter()
            .flatten()
            .min_by_key(|(at, _)| *at)
    }

    /// Whether the last turn left work for [`Runtime::poll_services`] that
    /// no thread will ring for: the scene changed since it last ran (a
    /// handler started a `ui.Timer`, activated a `Loader`), a model a view
    /// follows changed, a node waits to be torn down, a transform watcher
    /// owes its callback. The loop takes another turn at once for it rather
    /// than leaving it until something else wakes the shell.
    pub fn has_pending_work(&self) -> bool {
        let state = self.reactive.borrow();
        state.scene_revision != state.polled_revision
            || !state.retained_destroy_queue.is_empty()
            || state
                .transform_watchers
                .values()
                .any(|watcher| watcher.pending && watcher.callback.is_some())
            || state
                .views
                .values()
                .any(|view| view.model.borrow().has_changes())
    }
}
