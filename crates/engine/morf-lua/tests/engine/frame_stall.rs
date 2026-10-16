//! The frame-stall rehearsal (PLAN item 2, step 1): a busy handler and
//! a running spring in one runtime, driven frame by frame as a host turn
//! drives them -- handler first, then the motion tick, then the paint
//! that would follow. Motion is native and clock-driven, so the spring
//! keeps virtual time no matter what the handler costs; what the handler
//! costs is wall time in the turn, before the paint.

use std::time::{Duration, Instant};

use super::*;

const FRAMES: u32 = 150;
const STEP: Duration = Duration::from_millis(16);

fn rehearsal(busy: bool) -> (Option<u32>, Duration, Duration) {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "stall.lua",
            br#"
                local ui = require("morf.ui")
                local node = ui.Rect {
                    width = 10, height = 10,
                    behavior = { x = { kind = "spring", stiffness = 120, damping = 14 } },
                }
                morf.ipc.busy = function()
                    local sum = 0
                    for i = 1, 120000 do sum = sum + i end
                    return sum
                end
                morf.ipc.x = function() return node.x end
                node.x = 300
            "#,
        )
        .unwrap();
    let mut handler_total = Duration::ZERO;
    let mut settled = None;
    for frame in 0..FRAMES {
        if busy {
            let started = Instant::now();
            runtime.call_ipc("busy", &[]).unwrap();
            handler_total += started.elapsed();
        }
        runtime.poll_services();
        let tick = runtime.tick_frame_animations(STEP).unwrap();
        let x = match runtime.call_ipc("x", &[]).unwrap()[..] {
            [IpcValue::Number(x)] => x,
            [IpcValue::Integer(x)] => x as f64,
            ref other => panic!("unexpected x reply: {other:?}"),
        };
        if settled.is_none() && !tick.active && (x - 300.0).abs() < 0.5 {
            settled = Some(frame);
        }
    }
    (settled, handler_total, handler_total / FRAMES)
}

#[test]
fn a_busy_handler_stalls_the_turn_but_not_the_spring() {
    let (idle_settled, idle_total, _) = rehearsal(false);
    let (busy_settled, busy_total, busy_per_frame) = rehearsal(true);
    eprintln!(
        "frame-stall rehearsal: spring settled at frame {idle_settled:?} unstalled, \
         {busy_settled:?} stalled; busy handler {busy_per_frame:.2?}/frame, {busy_total:.2?} total"
    );
    // The spring keeps its schedule: motion is ticked by virtual time,
    // and the handler cannot delay it in frame count.
    assert_eq!(busy_settled, idle_settled);
    assert!(idle_settled.is_some(), "the spring never settled");
    // The handler's cost is real wall time in the turn, owned by the
    // handler stage: at 60Hz a frame is 16.7ms and this rehearsal's
    // handler is sized to be a visible share of one.
    assert!(
        busy_per_frame > Duration::from_millis(1),
        "busy handler too cheap to measure: {busy_per_frame:?}"
    );
    assert_eq!(idle_total, Duration::ZERO);
}
