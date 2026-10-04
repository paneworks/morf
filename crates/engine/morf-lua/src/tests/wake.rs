//! What a shell's loop sleeps until: the runtime's deadlines, the clock's
//! grain, and the work a turn leaves for the next.

use std::time::{Duration, Instant};

use crate::*;

fn runtime_with(source: &str) -> Runtime {
    let mut runtime = Runtime::default();
    runtime.execute("wake.lua", source.as_bytes()).unwrap();
    runtime.poll_services();
    runtime
}

#[test]
fn a_format_says_how_often_its_text_can_change() {
    assert_eq!(ClockPrecision::of_format("%H:%M"), ClockPrecision::Minutes);
    assert_eq!(
        ClockPrecision::of_format("%-I:%M %p"),
        ClockPrecision::Minutes
    );
    assert_eq!(ClockPrecision::of_format("%R"), ClockPrecision::Minutes);
    assert_eq!(
        ClockPrecision::of_format("%H:%M:%S"),
        ClockPrecision::Seconds
    );
    assert_eq!(ClockPrecision::of_format("%T"), ClockPrecision::Seconds);
    assert_eq!(ClockPrecision::of_format("%A %d %B"), ClockPrecision::Hours);
    assert_eq!(ClockPrecision::of_format("%Y-%m-%d"), ClockPrecision::Hours);
    assert_eq!(ClockPrecision::of_format("%H h"), ClockPrecision::Hours);
    assert_eq!(
        ClockPrecision::of_format("no time at all"),
        ClockPrecision::Hours
    );
    // Something this does not know is taken as the finest there is.
    assert_eq!(ClockPrecision::of_format("%H %K"), ClockPrecision::Seconds);
    assert_eq!(ClockPrecision::of_format("%.3f"), ClockPrecision::Seconds);
}

#[test]
fn the_clock_turns_over_within_its_grain() {
    for (precision, most) in [
        (ClockPrecision::Seconds, Duration::from_millis(1_002)),
        (ClockPrecision::Minutes, Duration::from_millis(60_002)),
        (ClockPrecision::Hours, Duration::from_millis(3_600_002)),
    ] {
        let until = precision.until_next();
        assert!(
            !until.is_zero() && until <= most,
            "{precision:?}: {until:?}"
        );
    }
}

#[test]
fn a_shell_that_shows_no_time_reads_no_clock_and_holds_no_deadline() {
    let runtime = runtime_with(
        r#"
        local ui = require("morf.ui")
        ui.Rect { width = 200, height = 60, color = "black" }
        "#,
    );
    assert_eq!(runtime.clock_precision(), None);
    assert!(runtime.next_deadline().is_none());
    assert!(!runtime.has_pending_work());
}

#[test]
fn the_clock_is_read_at_the_grain_the_bindings_show() {
    let seconds = runtime_with(
        r#"
        local ui = require("morf.ui")
        ui.Text { text = function() return morf.clock:get() end }
        "#,
    );
    assert_eq!(seconds.clock_precision(), Some(ClockPrecision::Seconds));

    let minutes = runtime_with(
        r#"
        local ui = require("morf.ui")
        ui.Text { text = function() return morf.minute_clock:get() end }
        "#,
    );
    assert_eq!(minutes.clock_precision(), Some(ClockPrecision::Minutes));

    // A clock made at seconds, formatted with no seconds in it.
    let formatted = runtime_with(
        r#"
        local ui = require("morf.ui")
        local core = require("morf.core")
        local clock = core.system_clock()
        ui.Text { text = function() return clock:format("%H:%M") end }
        "#,
    );
    assert_eq!(formatted.clock_precision(), Some(ClockPrecision::Minutes));

    let hours = runtime_with(
        r#"
        local ui = require("morf.ui")
        local core = require("morf.core")
        local clock = core.system_clock { precision = "hours" }
        ui.Text { text = function() return clock:format("%A") end }
        "#,
    );
    assert_eq!(hours.clock_precision(), Some(ClockPrecision::Hours));

    // The finest reader decides.
    let both = runtime_with(
        r#"
        local ui = require("morf.ui")
        local core = require("morf.core")
        local clock = core.system_clock { precision = "seconds" }
        ui.Text { text = function() return clock:format("%A") end }
        ui.Text { text = function() return clock:format("%S") end }
        "#,
    );
    assert_eq!(both.clock_precision(), Some(ClockPrecision::Seconds));
}

#[test]
fn a_minute_binding_runs_once_a_minute_however_often_the_clock_ticks() {
    let mut runtime = runtime_with(
        r#"
        local ui = require("morf.ui")
        ui.Text { text = function() return morf.minute_clock:get() end }
        "#,
    );
    runtime.update_clock("12:34:56").unwrap();
    let node = runtime.scene().roots()[0];
    assert_eq!(runtime.scene().string_value(node, "text").unwrap(), "12:34");
    let runs = runtime.effect_runs();
    for second in 57..60 {
        assert!(!runtime.update_clock(format!("12:34:{second}")).unwrap());
    }
    assert_eq!(runtime.effect_runs(), runs, "no minute turned over");
    assert!(runtime.update_clock("12:35:00").unwrap());
    assert_eq!(runtime.scene().string_value(node, "text").unwrap(), "12:35");
    assert_eq!(morf_clock(&runtime), "12:35:00");
}

fn morf_clock(runtime: &Runtime) -> String {
    let state = runtime.reactive.borrow();
    match state.reactive.values.get(&state.clocks.seconds) {
        Some(IpcValue::String(text)) => text.clone(),
        other => panic!("clock is {other:?}"),
    }
}

#[test]
fn a_timer_is_a_deadline_the_loop_sleeps_until() {
    let mut runtime = runtime_with(
        r#"
        local fired = 0
        morf.timer(150, function() fired = fired + 1 end, true)
        morf.ipc.fired = function() return fired end
        "#,
    );
    let asked = Instant::now();
    let (at, cause) = runtime.next_deadline().expect("a timer is due");
    assert_eq!(cause, DeadlineCause::Timer);
    let wait = at.saturating_duration_since(asked);
    assert!(wait <= Duration::from_millis(150), "{wait:?}");
    assert!(wait >= Duration::from_millis(100), "{wait:?}");
    // Not before it.
    runtime.poll_services();
    assert_eq!(fired(&mut runtime), 0);
    std::thread::sleep(at.saturating_duration_since(Instant::now()));
    runtime.poll_services();
    assert_eq!(fired(&mut runtime), 1, "due at its deadline");
    let (next, _) = runtime.next_deadline().expect("it repeats");
    assert!(next > at, "moved on by an interval");
}

fn fired(runtime: &mut Runtime) -> i64 {
    match runtime.call_ipc("fired", &[]).unwrap().as_slice() {
        [IpcValue::Integer(value)] => *value,
        other => panic!("fired answered {other:?}"),
    }
}

#[test]
fn a_ui_timer_started_by_a_handler_is_work_for_the_next_turn() {
    let mut runtime = runtime_with(
        r#"
        local ui = require("morf.ui")
        local timer = ui.Timer { interval = 50, running = false, on_triggered = function() end }
        ui.Item { timer }
        morf.ipc.start = function() timer.running = true end
        "#,
    );
    assert!(runtime.next_deadline().is_none());
    assert!(!runtime.has_pending_work());
    runtime.call_ipc("start", &[]).unwrap();
    // The handler only changed the scene; the timer exists once a turn has
    // looked at it, and that turn is owed now rather than at the next wake.
    assert!(runtime.has_pending_work());
    runtime.poll_services();
    assert!(!runtime.has_pending_work());
    assert_eq!(
        runtime.next_deadline().map(|(_, cause)| cause),
        Some(DeadlineCause::Timer)
    );
}
