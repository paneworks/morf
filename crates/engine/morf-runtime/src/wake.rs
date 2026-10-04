//! When the loop has to wake on its own, and why: the grains a clock is
//! read at, and the causes a deadline can have.

use morf_scene::reactive::{Graph, SignalId};
use morf_value::IpcValue;

use crate::reactive::Reactive;
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

/// `morf.clock` ("HH:MM:SS"), `morf.minute_clock` ("HH:MM") and
/// `morf.hour_clock` ("HH"): one signal per grain, so whatever changes by the
/// minute does not wake the shell every second.
#[derive(Clone, Copy, Debug)]
pub struct Clocks {
    pub seconds: SignalId,
    pub minutes: SignalId,
    pub hours: SignalId,
}

impl Clocks {
    /// The three signals, empty, in `graph`; with their values for the
    /// mirror.
    pub fn new(graph: &mut Graph<IpcValue>) -> (Self, [(SignalId, IpcValue); 3]) {
        let empty = IpcValue::String(String::new());
        let clocks = Self {
            seconds: graph.signal("morf.clock", empty.clone()),
            minutes: graph.signal("morf.minute_clock", empty.clone()),
            hours: graph.signal("morf.hour_clock", empty.clone()),
        };
        let values = [
            (clocks.seconds, empty.clone()),
            (clocks.minutes, empty.clone()),
            (clocks.hours, empty),
        ];
        (clocks, values)
    }

    /// The signal a reader at `precision` depends on.
    pub fn signal(&self, precision: ClockPrecision) -> SignalId {
        match precision {
            ClockPrecision::Seconds => self.seconds,
            ClockPrecision::Minutes => self.minutes,
            ClockPrecision::Hours => self.hours,
        }
    }

    /// Writes the time ("HH:MM:SS"), and the minute and hour clocks from it,
    /// only the grains that turned over. A value in any other shape is
    /// written as it is and nothing is derived. Returns whether anything was
    /// written: the graph then owes a flush.
    pub fn update(&self, reactive: &mut Reactive, value: String) -> Result<bool, String> {
        let derived = value
            .get(..5)
            .filter(|_| value.len() == 8 && value.as_bytes()[2] == b':')
            .map(|minutes| (minutes.to_owned(), value[..2].to_owned()));
        let mut writes = vec![(self.seconds, value)];
        if let Some((minutes, hours)) = derived {
            writes.push((self.minutes, minutes));
            writes.push((self.hours, hours));
        }
        let mut changed = false;
        for (signal, text) in writes {
            let text = IpcValue::String(text);
            if reactive.values.get(&signal) == Some(&text) {
                continue;
            }
            reactive
                .graph
                .as_mut()
                .ok_or_else(|| "reactive graph is already running".to_owned())?
                .write(signal, text.clone())
                .map_err(|error| error.to_string())?;
            reactive.values.insert(signal, text);
            changed = true;
        }
        Ok(changed)
    }

    /// The finest clock anything currently reads, or nothing when no binding
    /// shows the time: the grain the loop has to wake at for the clock.
    pub fn precision(&self, reactive: &Reactive) -> Option<ClockPrecision> {
        let graph = reactive.graph.as_ref()?;
        [
            (self.seconds, ClockPrecision::Seconds),
            (self.minutes, ClockPrecision::Minutes),
            (self.hours, ClockPrecision::Hours),
        ]
        .into_iter()
        .find(|(signal, _)| graph.has_subscribers(*signal))
        .map(|(_, precision)| precision)
    }
}

#[cfg(test)]
mod clock_tests {
    use super::*;

    #[test]
    fn only_the_grains_that_turned_over_are_written() {
        let mut graph = Graph::default();
        let (clocks, values) = Clocks::new(&mut graph);
        let mut reactive = Reactive {
            graph: Some(graph),
            values: values.into_iter().collect(),
            ..Reactive::default()
        };
        assert!(clocks.update(&mut reactive, "10:15:00".to_owned()).unwrap());
        let read = |reactive: &Reactive, signal| reactive.values[&signal].clone();
        assert_eq!(
            read(&reactive, clocks.minutes),
            IpcValue::String("10:15".to_owned())
        );
        assert_eq!(
            read(&reactive, clocks.hours),
            IpcValue::String("10".to_owned())
        );
        assert!(clocks.update(&mut reactive, "10:15:01".to_owned()).unwrap());
        assert!(!clocks.update(&mut reactive, "10:15:01".to_owned()).unwrap());
        assert!(clocks.update(&mut reactive, "soon".to_owned()).unwrap());
        assert_eq!(
            read(&reactive, clocks.minutes),
            IpcValue::String("10:15".to_owned())
        );
        assert_eq!(clocks.precision(&reactive), None, "nothing reads them");
        assert_eq!(clocks.signal(ClockPrecision::Hours), clocks.hours);
    }
}
