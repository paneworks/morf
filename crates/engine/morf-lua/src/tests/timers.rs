//! Timers that came due in a turn in which something else stopped them.

use std::thread;
use std::time::Duration;

use super::*;

fn integer(runtime: &mut Runtime, verb: &str) -> i64 {
    match runtime.call_ipc(verb, &[]).unwrap().as_slice() {
        [IpcValue::Integer(value)] => *value,
        other => panic!("{verb} answered {other:?}"),
    }
}

#[test]
fn unchanged_polls_preserve_timer_deadlines_and_native_changes_rearm_them() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "timer-cache.lua",
            br#"
        local ui = require("morf.ui")
        ui.Timer { interval=1000, running=true, ["repeat"]=true,
            on_triggered=function() end }
    "#,
        )
        .unwrap();
    let timer = runtime.scene().roots()[0];
    runtime.poll_services();
    let deadline = runtime.next_deadline().unwrap().0;
    for _ in 0..10 {
        runtime.poll_services();
    }
    assert_eq!(runtime.next_deadline().unwrap().0, deadline);
    runtime.scene_mut().assign(timer, "running", false).unwrap();
    runtime.poll_services();
    assert!(runtime.reactive.borrow().timers.is_empty());
    runtime.scene_mut().assign(timer, "running", true).unwrap();
    runtime.poll_services();
    assert_eq!(runtime.reactive.borrow().timers.len(), 1);
}

#[test]
fn a_hidden_timers_animated_interval_reconciles_on_its_final_tick() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "animated-timer.lua",
            br#"
        local ui=require("morf.ui")
        local timer=ui.Timer { interval=1000, running=true, ["repeat"]=true,
            on_triggered=function() end }
        ui.Item { visible=false, timer }
    "#,
        )
        .unwrap();
    runtime.poll_services();
    let root = runtime.scene().roots()[0];
    let timer = runtime.scene().children(root).unwrap()[0];
    runtime
        .scene_mut()
        .set_behavior(
            timer,
            "interval",
            Some(morf_scene::Behavior {
                duration: Duration::from_millis(10),
                ..morf_scene::Behavior::default()
            }),
        )
        .unwrap();
    runtime
        .scene_mut()
        .assign(timer, "interval", 2000.0)
        .unwrap();
    runtime.poll_services();
    runtime.tick_animations(Duration::from_millis(10)).unwrap();
    assert!(!runtime.has_motion());
    runtime.poll_services();
    assert_eq!(
        runtime.reactive.borrow().timers[0].interval,
        Duration::from_millis(2000)
    );
}

/// Polls until `done` holds, for at most a second.
fn poll_until(runtime: &mut Runtime, mut done: impl FnMut(&mut Runtime) -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while !done(runtime) && std::time::Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
        runtime.poll_services();
    }
    assert!(done(runtime), "timed out");
}

#[test]
fn a_timer_in_a_loader_does_not_fire_after_the_loader_let_it_go() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "loaded-timer.lua",
            br#"
                local ui = require("morf.ui")
                local active = morf.signal("active", true)
                local fired = 0
                ui.Loader {
                    active = function() return active:get() end,
                    source = function()
                        local label = ui.Text { text = "" }
                        return ui.Item {
                            label,
                            ui.Timer {
                                interval = 2,
                                ["repeat"] = true,
                                running = true,
                                on_triggered = function()
                                    fired = fired + 1
                                    label.text = tostring(fired)
                                end,
                            },
                        }
                    end,
                }
                morf.ipc.fired = function() return fired end
                morf.ipc.unload = function() active:set(false) end
            "#,
        )
        .unwrap();
    poll_until(&mut runtime, |runtime| integer(runtime, "fired") >= 1);
    runtime.take_logs();

    // Torn down, then left long enough that the timer is due again when the
    // turn that tears it down comes.
    runtime.call_ipc("unload", &[]).unwrap();
    let before = integer(&mut runtime, "fired");
    thread::sleep(Duration::from_millis(10));
    runtime.poll_services();
    thread::sleep(Duration::from_millis(10));
    runtime.poll_services();

    assert_eq!(integer(&mut runtime, "fired"), before);
    let logs = runtime.take_logs();
    assert!(logs.is_empty(), "{logs:?}");
}

#[test]
fn a_timer_stopped_earlier_in_the_same_turn_does_not_fire() {
    // Two timers due together, each stopping the other: whichever runs first
    // wins, and the other, though collected as due, stays quiet.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "rival-timers.lua",
            br#"
                local ui = require("morf.ui")
                local fired = 0
                local first, second
                first = ui.Timer {
                    interval = 5, ["repeat"] = true, running = false,
                    on_triggered = function() fired = fired + 1; second.running = false end,
                }
                second = ui.Timer {
                    interval = 5, ["repeat"] = true, running = false,
                    on_triggered = function() fired = fired + 1; first.running = false end,
                }
                ui.Item { first, second }
                morf.ipc.start = function() first.running = true; second.running = true end
                morf.ipc.fired = function() return fired end
            "#,
        )
        .unwrap();
    runtime.call_ipc("start", &[]).unwrap();
    // Arms both in one turn, so they come due together.
    runtime.poll_services();
    thread::sleep(Duration::from_millis(20));
    runtime.poll_services();

    assert_eq!(integer(&mut runtime, "fired"), 1);
}

#[test]
fn a_one_shot_cancelled_earlier_in_the_same_turn_does_not_fire() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "rival-one-shots.lua",
            br#"
                local fired = 0
                local first, second
                first = morf.timer(3, function() fired = fired + 1; second:cancel() end, false)
                second = morf.timer(3, function() fired = fired + 1; first:cancel() end, false)
                morf.ipc.fired = function() return fired end
            "#,
        )
        .unwrap();
    thread::sleep(Duration::from_millis(20));
    runtime.poll_services();
    thread::sleep(Duration::from_millis(20));
    runtime.poll_services();

    assert_eq!(integer(&mut runtime, "fired"), 1);
}

#[test]
fn a_stopped_timer_never_fires() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "stopped-timer.lua",
            br#"
                local ui = require("morf.ui")
                local fired = 0
                local timer = ui.Timer {
                    interval = 2, ["repeat"] = true, running = true,
                    on_triggered = function() fired = fired + 1 end,
                }
                ui.Item { timer }
                morf.ipc.stop = function() timer.running = false end
                morf.ipc.fired = function() return fired end
            "#,
        )
        .unwrap();
    poll_until(&mut runtime, |runtime| integer(runtime, "fired") >= 1);
    // Stopped while already due: the next turn must not run it.
    runtime.call_ipc("stop", &[]).unwrap();
    let before = integer(&mut runtime, "fired");
    thread::sleep(Duration::from_millis(10));
    runtime.poll_services();
    thread::sleep(Duration::from_millis(10));
    runtime.poll_services();

    assert_eq!(integer(&mut runtime, "fired"), before);
}
