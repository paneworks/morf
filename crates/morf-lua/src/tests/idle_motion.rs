//! Motion that repeats, waits, chains and ends with its node: `loops`,
//! `alternate`, `delay` and `sequence` on a group, and `loop` on a node.

use std::time::Duration;

use super::*;

fn tick(runtime: &mut Runtime, millis: u64) {
    runtime
        .tick_animations(Duration::from_millis(millis))
        .unwrap();
}

#[test]
fn a_group_alternates_after_a_delay_and_reports_its_end() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "bob.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local face = ui.Item { translate_y = 0 }
                _G.group = morf.animation.play {
                    delay = 100, loops = 2, alternate = true,
                    sequence = {
                        { node = face, property = "translate_y", to = -10, duration = 100 },
                    },
                    on_finished = function(reason) _G.reason = reason end,
                }
                morf.ipc.reason = function() return _G.reason or "" end
            "#,
        )
        .unwrap();
    let face = runtime.scene().roots()[0];
    tick(&mut runtime, 50);
    assert_eq!(runtime.scene().number(face, "translate_y").unwrap(), 0.0);
    tick(&mut runtime, 150);
    assert_eq!(
        runtime.scene().number(face, "translate_y").unwrap(),
        -10.0,
        "up after the wait"
    );
    tick(&mut runtime, 50);
    let y = runtime.scene().number(face, "translate_y").unwrap();
    assert!(y > -10.0 && y < 0.0, "and on its way back down: {y}");
    tick(&mut runtime, 60);
    tick(&mut runtime, 16);
    assert_eq!(runtime.scene().number(face, "translate_y").unwrap(), 0.0);
    assert_eq!(
        runtime.call_ipc("reason", &[]).unwrap(),
        [IpcValue::String("completed".to_owned())]
    );
}

#[test]
fn on_finished_can_chain_the_next_group() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "chain.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local card = ui.Item { x = 0, y = 0 }
                morf.animation.play {
                    { node = card, property = "x", to = 10, duration = 50 },
                    on_finished = function()
                        _G.second = morf.animation.play {
                            { node = card, property = "y", to = 20, duration = 50 },
                        }
                    end,
                }
            "#,
        )
        .unwrap();
    let card = runtime.scene().roots()[0];
    tick(&mut runtime, 60);
    tick(&mut runtime, 16);
    assert_eq!(runtime.scene().number(card, "x").unwrap(), 10.0);
    tick(&mut runtime, 60);
    assert_eq!(runtime.scene().number(card, "y").unwrap(), 20.0);
}

#[test]
fn a_group_whose_node_is_destroyed_ends_as_canceled() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "cancel.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local dot = ui.Item { opacity = 1 }
                _G.group = morf.animation.play {
                    loops = "forever",
                    { node = dot, property = "opacity", to = 0, duration = 100 },
                    on_finished = function(reason) _G.reason = reason end,
                }
                morf.ipc.destroy = function() ui.destroy(dot) end
                morf.ipc.state = function()
                    return (_G.reason or "") .. "/" .. tostring(_G.group:active())
                end
            "#,
        )
        .unwrap();
    tick(&mut runtime, 30);
    runtime.call_ipc("destroy", &[]).unwrap();
    tick(&mut runtime, 16);
    assert_eq!(
        runtime.call_ipc("state", &[]).unwrap(),
        [IpcValue::String("canceled/false".to_owned())]
    );
}

#[test]
fn a_node_loop_runs_until_its_binding_lets_it_go() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "loop.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local awake = morf.signal("awake", true)
                ui.Item {
                    translate_y = 0,
                    loop = function()
                        if not awake:get() then return nil end
                        return { translate_y = { from = 0, to = -4, duration = 100,
                                                 easing = "in_out_sine", alternate = true } }
                    end,
                }
                morf.ipc.sleep = function() awake:set(false) end
            "#,
        )
        .unwrap();
    let face = runtime.scene().roots()[0];
    assert!(runtime.scene().is_animating(face, "translate_y").unwrap());
    // Up, down, up again: it does not settle.
    tick(&mut runtime, 100);
    assert!((runtime.scene().number(face, "translate_y").unwrap() + 4.0).abs() < 0.01);
    tick(&mut runtime, 100);
    assert!(runtime.scene().number(face, "translate_y").unwrap().abs() < 0.01);
    tick(&mut runtime, 1_050);
    assert!(runtime.scene().is_animating(face, "translate_y").unwrap());
    let y = runtime.scene().number(face, "translate_y").unwrap();
    assert!(y < -0.5, "still bobbing: {y}");

    // Asleep: the loop ends and the property goes back to where it began.
    runtime.call_ipc("sleep", &[]).unwrap();
    assert!(!runtime.scene().is_animating(face, "translate_y").unwrap());
    assert_eq!(runtime.scene().number(face, "translate_y").unwrap(), 0.0);
}

#[test]
fn a_held_loop_stays_where_it_stopped() {
    // A spinner that stops spinning keeps the angle it had, and one started
    // again turns on from there rather than jumping back to its `from`.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "hold.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local spinning = morf.signal("spinning", true)
                ui.Item {
                    loop = function()
                        if not spinning:get() then return nil end
                        return { rotation = { to = 360, duration = 400, hold = true } }
                    end,
                }
                morf.ipc.spin = function(on) spinning:set(on) end
                assert(not pcall(ui.Item, { loop = { x = { to = 1, duration = 10, hold = 1 } } }))
            "#,
        )
        .unwrap();
    let face = runtime.scene().roots()[0];
    tick(&mut runtime, 100);
    runtime
        .call_ipc("spin", &[IpcValue::Boolean(false)])
        .unwrap();
    assert!(!runtime.scene().is_animating(face, "rotation").unwrap());
    let stopped = runtime.scene().number(face, "rotation").unwrap();
    assert!((stopped - 90.0).abs() < 1.0, "held at {stopped}");
    tick(&mut runtime, 200);
    assert_eq!(runtime.scene().number(face, "rotation").unwrap(), stopped);
    assert_eq!(
        runtime.scene().target(face, "rotation").unwrap(),
        &morf_scene::Value::Number(stopped)
    );
    runtime
        .call_ipc("spin", &[IpcValue::Boolean(true)])
        .unwrap();
    tick(&mut runtime, 16);
    let resumed = runtime.scene().number(face, "rotation").unwrap();
    assert!(
        resumed >= stopped && resumed < stopped + 20.0,
        "turns on from {stopped}: {resumed}"
    );
}

#[test]
fn a_static_loop_ends_with_its_node_and_bad_loops_are_refused() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "spin.lua",
            br#"
                local ui = require("morf.ui")
                _G.spinner = ui.Item { loop = { rotation = { from = 0, to = 360, duration = 800 } } }
                ui.Item { _G.spinner }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let spinner = runtime.scene().children(root).unwrap()[0];
    tick(&mut runtime, 200);
    let turned = runtime.scene().number(spinner, "rotation").unwrap();
    assert!((turned - 90.0).abs() < 1.0, "{turned}");
    runtime
        .execute("gone.lua", b"require('morf.ui').destroy(_G.spinner)")
        .unwrap();
    assert!(!runtime.scene().contains(spinner));
    tick(&mut runtime, 16);

    for (source, message) in [
        (
            "require('morf.ui').Item { loop = { x = { to = 1 } } }",
            "needs a duration",
        ),
        (
            "require('morf.ui').Item { loop = { x = { to = 1, duration = 10, bounce = 2 } } }",
            "no field `bounce`",
        ),
        (
            "require('morf.ui').Item { loop = { nope = { to = 1, duration = 10 } } }",
            "nope",
        ),
    ] {
        let error = runtime.execute("bad.lua", source.as_bytes()).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
}
