//! Tests of timers.rs that reach the runtime's internals; the rest are in
//! tests/engine/timers.rs.
#![allow(unused_imports)]

use super::*;
use std::thread;
use std::time::Duration;

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
        runtime
            .reactive
            .borrow()
            .timers
            .iter()
            .next()
            .unwrap()
            .interval,
        Duration::from_millis(2000)
    );
}
