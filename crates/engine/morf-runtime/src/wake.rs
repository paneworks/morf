//! When the loop has to wake on its own, and why: the grains a clock is
//! read at, and the causes a deadline can have.

use std::time::{Duration, Instant};

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
    pub fn parse(name: &str) -> Self {
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
    /// A press held still becomes a long press.
    LongPress,
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
            Self::LongPress => "long-press",
        }
    }
}

/// The earliest of `deadlines`, with its cause.
pub fn earliest(
    deadlines: impl IntoIterator<Item = Option<(Instant, DeadlineCause)>>,
) -> Option<(Instant, DeadlineCause)> {
    deadlines.into_iter().flatten().min_by_key(|(at, _)| *at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_format_changes_as_often_as_its_finest_conversion() {
        assert_eq!(ClockPrecision::of_format("%H:%M"), ClockPrecision::Minutes);
        assert_eq!(
            ClockPrecision::of_format("%A %-d %B"),
            ClockPrecision::Hours
        );
        assert_eq!(
            ClockPrecision::of_format("%H:%M:%S"),
            ClockPrecision::Seconds
        );
        assert_eq!(ClockPrecision::of_format("%.3f"), ClockPrecision::Seconds);
    }
}
