// A turn owed only for frame callbacks paints before its timers and
// services run, and loses nothing: the timer fires in the same turn, what
// it changed is painted on the next, and input still takes the full turn.

use std::path::PathBuf;
use std::time::Duration;

use morf_app::{Backend, PRIMARY_LAYER, WindowId};
use morf_host::headless::{Headless, LoadOptions};
use morf_host::host::turn::Turn;
use morf_value::IpcValue;

const FRAME: Duration = Duration::from_millis(16);

const SPRING: &str = r#"
    local ui = require("morf.ui")
    morf.surface.anchors = {}
    morf.surface.width = 400
    morf.surface.height = 100
    local fired, clicks = 0, 0
    local box = ui.Rect {
        width = 10, height = 10, translate_x = 0,
        behavior = { translate_x = { kind = "spring", stiffness = 120, damping = 14 } },
    }
    ui.Item {
        anchors = { fill = true },
        box,
        ui.MouseArea {
            x = 0, y = 50, width = 400, height = 50,
            on_clicked = function() clicks = clicks + 1 end,
        },
    }
    morf.ipc.go = function() box.translate_x = 300 end
    morf.ipc.x = function() return box.translate_x end
    morf.ipc.fired = function() return fired end
    morf.ipc.clicks = function() return clicks end
    morf.timer(48, function() fired = fired + 1 box.width = 10 + fired end, false)
"#;

const STILL: &str = r##"
    local ui = require("morf.ui")
    morf.surface.anchors = {}
    morf.surface.width = 100
    morf.surface.height = 100
    local fired = 0
    local face = ui.Rect { width = 50, height = 50, color = "#000000" }
    ui.Item { anchors = { fill = true }, face }
    morf.ipc.fired = function() return fired end
    morf.timer(48, function() fired = fired + 1 face.color = "#ff0000" end, false)
"##;

fn load_source(source: &str) -> Headless {
    let mut options = LoadOptions::new(PathBuf::from("motion-turn.lua"));
    options.source = Some(source.as_bytes().to_vec());
    options.size = (800, 600);
    Headless::load(&options).unwrap_or_else(|failure| panic!("{}", failure.error))
}

fn load() -> Headless {
    let mut headless = load_source(SPRING);
    headless.runtime.call_ipc("go", &[]).unwrap();
    headless
}

fn number(headless: &mut Headless, verb: &str) -> f64 {
    match headless.runtime.call_ipc(verb, &[]).unwrap()[..] {
        [IpcValue::Integer(value)] => value as f64,
        [IpcValue::Number(value)] => value,
        ref other => panic!("{verb}: {other:?}"),
    }
}

/// One frame callback, paced as a compositor paces it, and one turn.
fn callback_turn(headless: &mut Headless) -> Turn {
    let Headless { host, runtime, .. } = headless;
    let host = host.as_mut().expect("a host");
    let backend = host.backend.as_headless_mut().expect("a headless backend");
    backend.request_frame(WindowId::Layer(PRIMARY_LAYER));
    backend.advance(FRAME);
    runtime.advance_virtual_clock(FRAME);
    host.turn(runtime, None).unwrap()
}

/// Turns while the host asks for one, as the loop does between callbacks.
fn follow_ups(headless: &mut Headless) -> usize {
    let Headless { host, runtime, .. } = headless;
    let host = host.as_mut().expect("a host");
    let mut turns = 0;
    while !host.settled() && turns < 8 {
        host.turn(runtime, None).unwrap();
        turns += 1;
    }
    turns
}

/// Whether the shell's own surface was last painted with the scene's
/// current layout.
fn painted_current(headless: &Headless) -> bool {
    let host = headless.host.as_ref().expect("a host");
    host.state.layout.revision
        == headless
            .runtime
            .scene()
            .layout_revision_of(host.state.primary_root)
}

fn paced(headless: &mut Headless) {
    let host = headless.host.as_mut().expect("a host");
    host.backend.as_headless_mut().unwrap().set_paced(true);
    follow_ups(headless);
    host_report(headless);
}

fn host_report(headless: &mut Headless) -> morf_host::host::turn::FrameReport {
    headless.host.as_mut().unwrap().take_frame_report()
}

#[test]
fn a_timer_due_on_a_motion_frame_fires_after_its_paint_and_is_painted_next_frame() {
    let mut headless = load();
    paced(&mut headless);
    for _ in 0..2 {
        callback_turn(&mut headless);
        follow_ups(&mut headless);
    }
    assert_eq!(number(&mut headless, "fired"), 0.0);
    // The frame the timer is due on: motion only, so the paint comes first
    // and the timer after it, in the same turn.
    assert_eq!(callback_turn(&mut headless), Turn::Again);
    assert_eq!(number(&mut headless, "fired"), 1.0, "the timer fired");
    assert!(!painted_current(&headless), "painted before the timer ran");
    let host = headless.host.as_ref().unwrap();
    assert!(
        host.wants_turn(),
        "the repaint the timer owes asks for a turn"
    );
    // Owed, and held for the callback the paint asked for.
    follow_ups(&mut headless);
    assert!(!painted_current(&headless));
    assert!(headless.host.as_ref().unwrap().state.primary_deferred);
    callback_turn(&mut headless);
    assert!(painted_current(&headless), "the next frame painted it");
    let report = host_report(&mut headless);
    assert!(report.motion_turns >= 4, "{report}");
    assert!(report.frames >= 4, "{report}");
}

#[test]
fn a_paint_a_timer_owes_after_a_still_frame_is_not_lost() {
    let mut headless = load_source(STILL);
    paced(&mut headless);
    for _ in 0..2 {
        callback_turn(&mut headless);
        follow_ups(&mut headless);
    }
    host_report(&mut headless);
    // Only a callback, with nothing moving: the timer runs after a frame
    // that painted nothing, and its colour is drawn by the turn after.
    callback_turn(&mut headless);
    assert_eq!(number(&mut headless, "fired"), 1.0);
    follow_ups(&mut headless);
    let report = host_report(&mut headless);
    assert_eq!((report.motion_turns, report.frames), (1, 1), "{report}");
}

#[test]
fn a_service_thread_ringing_takes_the_full_turn() {
    let mut headless = load();
    paced(&mut headless);
    headless.host.as_mut().unwrap().rang(1);
    callback_turn(&mut headless);
    assert_eq!(host_report(&mut headless).motion_turns, 0);
    headless.host.as_mut().unwrap().rang(1);
    callback_turn(&mut headless);
    assert_eq!(host_report(&mut headless).motion_turns, 1);
}

#[test]
fn without_frame_pacing_the_owed_repaint_lands_in_the_same_frame() {
    let mut headless = load();
    headless.advance(Duration::from_millis(48), Duration::ZERO);
    assert_eq!(number(&mut headless, "fired"), 1.0);
    assert!(painted_current(&headless));
    assert!(host_report(&mut headless).motion_turns > 0);
}

#[test]
fn input_during_motion_takes_the_full_turn_and_motion_goes_on() {
    let mut headless = load();
    paced(&mut headless);
    callback_turn(&mut headless);
    follow_ups(&mut headless);
    let moving_from = number(&mut headless, "x");
    let surface = WindowId::Layer(PRIMARY_LAYER);
    headless
        .click(surface, (200.0, 75.0), 0x110, Default::default())
        .unwrap();
    assert_eq!(number(&mut headless, "clicks"), 1.0);
    for _ in 0..4 {
        callback_turn(&mut headless);
        follow_ups(&mut headless);
    }
    assert!(
        number(&mut headless, "x") > moving_from,
        "the spring kept moving"
    );
    let report = host_report(&mut headless);
    assert!(report.turns > report.motion_turns, "{report}");
}

#[test]
fn a_spring_keeps_its_schedule_through_motion_turns() {
    let run = |paced_frames: bool| {
        let mut headless = load();
        if paced_frames {
            paced(&mut headless);
        }
        (0..40)
            .map(|_| {
                callback_turn(&mut headless);
                follow_ups(&mut headless);
                number(&mut headless, "x")
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(run(true), run(false));
}
