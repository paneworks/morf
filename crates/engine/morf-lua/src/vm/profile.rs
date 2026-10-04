//! `MORF_PROFILE=1`: where the time of a slow turn went.
//!
//! A turn of the loop that holds the output -- "services, timers and
//! callbacks took 120 ms" -- says nothing about *whose* work that was. With
//! the profiler on, every piece of Lua the engine runs is a span named after
//! what it is and where it came from (`effect impasto.island.landing
//! (bar/island.lua:198)`, `binding Item > ClipRect #island.width`, `timer
//! bar/island.lua:205`, `loader … (bar/island.lua:260)`, `ipc controls`), and
//! so is the engine's own bookkeeping between them (`engine: …`). Spans nest;
//! each is charged its *own* time, what its children took subtracted, so a
//! timer that ran a flush that ran forty bindings shows the bindings, not the
//! timer, as the cost. When a stage turns out slow, the caller takes the
//! report and prints its top entries; either way it clears it for the next.
//!
//! One profile per thread, as there is one runtime per output thread. Off,
//! a span costs one thread-local read and builds no label.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// One label's share of what was recorded since the last clear.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileEntry {
    /// What ran: a kind and where it came from.
    pub label: String,
    /// Time spent in it, its children's time excluded.
    pub own: Duration,
    /// Time spent in it, children included.
    pub total: Duration,
    /// How many times it ran.
    pub count: u32,
}

#[derive(Default)]
struct Profile {
    stack: Vec<(Instant, Duration)>,
    entries: HashMap<String, (Duration, Duration, u32)>,
}

thread_local! {
    static PROFILE: RefCell<Profile> = RefCell::new(Profile::default());
    static FORCED: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Whether profiling is on: `MORF_PROFILE` set to anything but `0`, or
/// [`set_enabled`] on this thread.
pub fn enabled() -> bool {
    static FROM_ENV: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    FORCED.with(Cell::get).unwrap_or_else(|| {
        *FROM_ENV.get_or_init(|| {
            std::env::var_os("MORF_PROFILE").is_some_and(|value| !value.is_empty() && value != "0")
        })
    })
}

/// Turns profiling on or off for this thread, whatever the environment says.
pub fn set_enabled(on: bool) {
    FORCED.with(|forced| forced.set(Some(on)));
}

/// A running span; it is charged when dropped.
#[must_use = "a span measures until it is dropped"]
pub struct Span {
    label: Option<String>,
}

/// Starts a span named by `label`, which is only built when profiling is on.
pub fn span(label: impl FnOnce() -> String) -> Span {
    if !enabled() {
        return Span { label: None };
    }
    let label = label();
    PROFILE.with(|profile| {
        profile
            .borrow_mut()
            .stack
            .push((Instant::now(), Duration::ZERO))
    });
    Span { label: Some(label) }
}

impl Drop for Span {
    fn drop(&mut self) {
        let Some(label) = self.label.take() else {
            return;
        };
        PROFILE.with(|profile| {
            let mut profile = profile.borrow_mut();
            let Some((started, children)) = profile.stack.pop() else {
                return;
            };
            let total = started.elapsed();
            let own = total.saturating_sub(children);
            if let Some(parent) = profile.stack.last_mut() {
                parent.1 += total;
            }
            let entry = profile
                .entries
                .entry(label)
                .or_insert((Duration::ZERO, Duration::ZERO, 0));
            entry.0 += own;
            entry.1 += total;
            entry.2 += 1;
        });
    }
}

/// Everything recorded since the last clear, the costliest (own time) first,
/// and clears it. Spans still open keep running.
pub fn take() -> Vec<ProfileEntry> {
    PROFILE.with(|profile| {
        let mut entries: Vec<ProfileEntry> = profile
            .borrow_mut()
            .entries
            .drain()
            .map(|(label, (own, total, count))| ProfileEntry {
                label,
                own,
                total,
                count,
            })
            .collect();
        entries.sort_by(|a, b| b.own.cmp(&a.own).then_with(|| a.label.cmp(&b.label)));
        entries
    })
}

/// Forgets what was recorded.
pub fn clear() {
    if enabled() {
        PROFILE.with(|profile| profile.borrow_mut().entries.clear());
    }
}

/// The top `max` entries of [`take`] as lines for a log, or nothing when
/// profiling is off.
pub fn report(max: usize) -> Vec<String> {
    if !enabled() {
        return Vec::new();
    }
    let entries = take();
    let hidden = entries.len().saturating_sub(max);
    let mut lines: Vec<String> = entries
        .into_iter()
        .take(max)
        .map(|entry| {
            format!(
                "{:7.2} ms own {:7.2} ms total {:4}x  {}",
                entry.own.as_secs_f64() * 1000.0,
                entry.total.as_secs_f64() * 1000.0,
                entry.count,
                entry.label
            )
        })
        .collect();
    if hidden > 0 {
        lines.push(format!("… and {hidden} more"));
    }
    lines
}

/// Milliseconds on the system's monotonic clock (`CLOCK_MONOTONIC`, what
/// Python's `time.monotonic()` and the compositor's frame times count), for
/// the diagnostic logs: a line stamped with it can be lined up with a screen
/// recording, or another process's log, taken on the same clock.
pub fn monotonic_ms() -> f64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `now` is a valid, writable timespec for the call's duration.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    now.tv_sec as f64 * 1000.0 + now.tv_nsec as f64 / 1_000_000.0
}

/// The stamp every diagnostic line starts with: `[12345.678]`, monotonic ms.
pub fn stamp() -> String {
    format!("[{:.3}]", monotonic_ms())
}

/// Where a Lua function was written, `chunk:line`, for a label.
pub(crate) fn closure_origin(closure: luna::Closure<'_>) -> String {
    let prototype = closure.prototype();
    let line = prototype
        .opcode_line_numbers
        .first()
        .map(|(_, line)| line.to_string())
        .unwrap_or_else(|| "?".to_owned());
    format!(
        "{}:{line}",
        short_chunk(&prototype.chunk_name.display_lossy().to_string())
    )
}

/// A chunk name without the directories above the configuration, which are
/// the same for every line of a report.
fn short_chunk(name: &str) -> String {
    let name = name.trim_start_matches('@');
    let parts: Vec<&str> = name.rsplit('/').take(2).collect();
    parts.into_iter().rev().collect::<Vec<_>>().join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_spans_charge_their_own_time() {
        set_enabled(true);
        let _ = take();
        {
            let _outer = span(|| "outer".to_owned());
            std::thread::sleep(Duration::from_millis(4));
            {
                let _inner = span(|| "inner".to_owned());
                std::thread::sleep(Duration::from_millis(12));
            }
        }
        let entries = take();
        assert_eq!(entries[0].label, "inner");
        assert!(entries[0].own >= Duration::from_millis(12));
        let outer = entries.iter().find(|entry| entry.label == "outer").unwrap();
        assert!(outer.total >= Duration::from_millis(16));
        assert!(outer.own < Duration::from_millis(12));
        assert!(take().is_empty());
        set_enabled(false);
    }

    #[test]
    fn off_records_nothing_and_builds_no_label() {
        set_enabled(false);
        let _span = span(|| panic!("a label built while off"));
        drop(_span);
        set_enabled(true);
        assert!(take().is_empty());
        set_enabled(false);
    }

    #[test]
    fn repeats_are_counted_under_one_label() {
        set_enabled(true);
        let _ = take();
        for _ in 0..3 {
            let _span = span(|| "timer a.lua:3".to_owned());
        }
        let entries = take();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].count, 3);
        assert_eq!(report(5).len(), 0);
        set_enabled(false);
    }

    #[test]
    fn chunk_names_keep_the_last_directory() {
        assert_eq!(
            short_chunk("@/home/x/impasto/bar/island.lua"),
            "bar/island.lua"
        );
        assert_eq!(short_chunk("init.lua"), "init.lua");
    }
}
