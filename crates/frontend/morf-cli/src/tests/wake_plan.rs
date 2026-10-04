//! What an output's loop sleeps until, and what rings it.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use morf_lua::{ClockPrecision, DeadlineCause, Runtime};

use morf_host::lock::{WorkerCommand, WorkerSender};
use morf_host::wake_plan::{Reason, Sleep};

fn runtime_with(source: &str) -> Runtime {
    let mut runtime = Runtime::default();
    runtime.execute("plan.lua", source.as_bytes()).unwrap();
    runtime.poll_services();
    runtime
}

#[test]
fn an_idle_shell_sleeps_until_something_happens() {
    let runtime = runtime_with(
        r#"
        local ui = require("morf.ui")
        ui.Rect { width = 200, height = 60, color = "black" }
        "#,
    );
    let sleep = Sleep::plan(&runtime, false, None);
    assert!(sleep.deadline.is_none(), "{:?}", sleep.deadline);
    assert_eq!(sleep.timeout(), None);
}

#[test]
fn a_turn_that_left_work_is_followed_by_another_at_once() {
    let runtime = runtime_with("");
    let sleep = Sleep::plan(&runtime, true, None);
    assert_eq!(
        sleep.deadline.map(|(_, reason)| reason),
        Some(Reason::Pending)
    );
    assert_eq!(sleep.timeout(), Some(Duration::ZERO));
}

#[test]
fn the_clock_wakes_the_loop_at_the_grain_it_is_read() {
    let seconds = runtime_with(
        r#"
        local ui = require("morf.ui")
        ui.Text { text = function() return morf.clock:get() end }
        "#,
    );
    let sleep = Sleep::plan(&seconds, false, None);
    assert_eq!(
        sleep.deadline.map(|(_, reason)| reason),
        Some(Reason::Clock(ClockPrecision::Seconds))
    );
    assert!(sleep.timeout().unwrap() <= Duration::from_millis(1_002));

    let minutes = runtime_with(
        r#"
        local ui = require("morf.ui")
        local clock = require("morf.core").system_clock { precision = "minutes" }
        ui.Text { text = function() return clock:format("%H:%M") end }
        "#,
    );
    let sleep = Sleep::plan(&minutes, false, None);
    assert_eq!(
        sleep.deadline.map(|(_, reason)| reason),
        Some(Reason::Clock(ClockPrecision::Minutes))
    );
    assert!(sleep.timeout().unwrap() <= Duration::from_millis(60_002));
}

#[test]
fn the_earliest_deadline_wins() {
    let runtime = runtime_with(
        r#"
        local ui = require("morf.ui")
        ui.Text { text = function() return morf.clock:get() end }
        morf.timer(5, function() end, true)
        "#,
    );
    let sleep = Sleep::plan(&runtime, false, None);
    assert_eq!(
        sleep.deadline.map(|(_, reason)| reason),
        Some(Reason::Runtime(DeadlineCause::Timer))
    );
    let soon = std::time::Instant::now();
    let sleep = Sleep::plan(&runtime, false, Some(soon));
    assert_eq!(sleep.deadline, Some((soon, Reason::Fallback)));
}

#[test]
fn a_command_for_an_output_rings_its_loop() {
    // The output thread sleeps until something rings: an IPC call forwarded
    // to it has to, or it waits for an unrelated wake and its caller times
    // out first.
    let wake = morf_io::Wake::new().unwrap();
    wake.drain();
    let (sender, receiver) = mpsc::channel();
    let sender = WorkerSender::new(sender);
    let (reply, _answer) = mpsc::sync_channel(1);
    sender.send(WorkerCommand::Logs(reply)).unwrap();
    assert!(
        wake.wait(Duration::from_secs(1)),
        "the command rang the loop"
    );
    assert!(receiver.try_recv().is_ok());
}

#[test]
fn a_paint_owed_on_an_overdue_frame_callback_is_made_once_a_stall() {
    // A compositor that draws only damage may never answer the callback an
    // empty commit asked for; the change the shell owes a paint for then
    // showed only after the next unrelated event. Past a stall it is painted
    // anyway -- and not again until another stall has passed.
    use morf_host::surfaces::{frame_stall, owed_paint_due};
    let refresh = Duration::from_millis(16);
    let stall = frame_stall(refresh);
    let now = Instant::now();
    let overdue = Some(stall + Duration::from_millis(1));
    assert!(
        !owed_paint_due(false, overdue, refresh, None, now),
        "nothing owed"
    );
    assert!(
        !owed_paint_due(true, None, refresh, None, now),
        "no callback outstanding: the loop paints as usual"
    );
    assert!(
        !owed_paint_due(true, Some(stall / 2), refresh, None, now),
        "not overdue yet"
    );
    assert!(owed_paint_due(true, overdue, refresh, None, now));
    assert!(
        !owed_paint_due(true, overdue, refresh, Some(now), now),
        "just forced one"
    );
    assert!(owed_paint_due(
        true,
        overdue,
        refresh,
        Some(now - stall * 2),
        now
    ));
}
