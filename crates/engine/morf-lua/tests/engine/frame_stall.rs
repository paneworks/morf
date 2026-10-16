//! Where a busy configuration's time is charged: a timer's own Lua to the
//! handlers stage, the bindings its write re-runs to the flush, the tick
//! to motion. What the host's frame accounting reads, per stage
//! (`morf_lua::profile::take_stages`).

use std::time::Duration;

use morf_lua::profile::{Stage, StageTimes, take_stages};

use super::*;

/// Fires every timer due 16 ms on, and says what each stage was charged.
fn fire(runtime: &mut Runtime) -> StageTimes {
    runtime.advance_virtual_clock(Duration::from_millis(16));
    let _ = take_stages();
    runtime.poll_services();
    take_stages()
}

fn charged(times: &StageTimes, stage: Stage) -> Duration {
    times[stage as usize]
}

#[test]
fn a_busy_timer_is_charged_to_handlers_and_not_to_the_flush() {
    let mut runtime = Runtime::default();
    runtime.use_virtual_clock();
    runtime
        .execute(
            "busy-timer.lua",
            br#"
                local fired = 0
                morf.ipc.fired = function() return fired end
                morf.timer(16, function()
                    local sum = 0
                    for i = 1, 200000 do sum = sum + i end
                    fired = fired + 1
                end)
            "#,
        )
        .unwrap();
    let times = fire(&mut runtime);
    assert_eq!(
        runtime.call_ipc("fired", &[]).unwrap(),
        [IpcValue::Integer(1)]
    );
    let handlers = charged(&times, Stage::Handlers);
    assert!(handlers > Duration::ZERO, "{times:?}");
    // It wrote nothing a binding reads, so nothing flushed.
    assert_eq!(charged(&times, Stage::Flush), Duration::ZERO, "{times:?}");
    assert!(handlers > charged(&times, Stage::Services) * 4, "{times:?}");
}

#[test]
fn bindings_a_timer_wakes_are_charged_to_the_flush() {
    let mut runtime = Runtime::default();
    runtime.use_virtual_clock();
    runtime
        .execute(
            "busy-binding.lua",
            br#"
                local ui = require("morf.ui")
                local tick = morf.signal("busy.tick", 0)
                ui.Text {
                    text = function()
                        local sum = 0
                        for i = 1, 200000 do sum = sum + i end
                        return tostring(tick:get() + sum % 2)
                    end,
                }
                morf.timer(16, function() tick:set(tick:get() + 1) end)
            "#,
        )
        .unwrap();
    let times = fire(&mut runtime);
    let flush = charged(&times, Stage::Flush);
    assert!(flush > charged(&times, Stage::Handlers) * 4, "{times:?}");
    assert!(flush > charged(&times, Stage::Services) * 4, "{times:?}");
}

#[test]
fn the_animation_tick_is_charged_to_motion() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "spring.lua",
            br#"
                local ui = require("morf.ui")
                local nodes = {}
                for index = 1, 400 do
                    nodes[index] = ui.Rect {
                        width = 10, height = 10,
                        behavior = { x = { kind = "spring", stiffness = 120, damping = 14 } },
                    }
                end
                for _, node in ipairs(nodes) do node.x = 300 end
            "#,
        )
        .unwrap();
    let _ = take_stages();
    let frame = runtime
        .tick_frame_animations(Duration::from_millis(16))
        .unwrap();
    let times = take_stages();
    assert!(frame.active);
    assert!(charged(&times, Stage::Motion) > Duration::ZERO, "{times:?}");
    assert_eq!(
        charged(&times, Stage::Handlers),
        Duration::ZERO,
        "{times:?}"
    );
}
