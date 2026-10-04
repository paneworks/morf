//! Tests of wake.rs that reach the runtime's internals; the rest are in
//! tests/engine/wake.rs.
#![allow(unused_imports)]

use super::*;
use crate::*;
use std::time::{Duration, Instant};

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

fn runtime_with(source: &str) -> Runtime {
    let mut runtime = Runtime::default();
    runtime.execute("wake.lua", source.as_bytes()).unwrap();
    runtime.poll_services();
    runtime
}
