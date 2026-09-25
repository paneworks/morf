//! `exit`: a node let go of plays its way out before it is removed.

use std::time::Duration;

use super::*;

fn text(runtime: &mut Runtime, verb: &str) -> String {
    match &runtime.call_ipc(verb, &[]).unwrap()[..] {
        [IpcValue::String(text)] => text.clone(),
        other => panic!("{verb}: {other:?}"),
    }
}

fn number(runtime: &mut Runtime, verb: &str) -> f64 {
    match &runtime.call_ipc(verb, &[]).unwrap()[..] {
        [IpcValue::Number(value)] => *value,
        [IpcValue::Integer(value)] => *value as f64,
        other => panic!("{verb}: {other:?}"),
    }
}

fn tick(runtime: &mut Runtime, ms: u64) {
    runtime.tick_animations(Duration::from_millis(ms)).unwrap();
}

const LOADER: &[u8] = br#"
    local morf = require("morf")
    local ui = require("morf.ui")
    local active = morf.signal("active", true)
    _G.built, _G.gone = 0, 0
    _G.page = nil
    ui.Column {
        ui.Loader {
            active = function() return active:get() end,
            source = function()
                _G.built = _G.built + 1
                _G.page = ui.Rect {
                    width = 40, height = 20, opacity = 1,
                    exit = { opacity = 0, scale = 0.5, duration = 100, easing = "linear" },
                    on_destroyed = function() _G.gone = _G.gone + 1 end,
                }
                return _G.page
            end,
        },
    }
    morf.ipc.open = function() active:set(true) end
    morf.ipc.close = function() active:set(false) end
    morf.ipc.state = function()
        return ("built %d gone %d"):format(_G.built, _G.gone)
    end
    morf.ipc.opacity = function() return _G.page.opacity end
"#;

#[test]
fn a_loader_let_go_plays_its_items_exit_then_removes_it() {
    let mut runtime = Runtime::default();
    runtime.execute("loader.lua", LOADER).unwrap();
    runtime.poll_services();
    assert_eq!(text(&mut runtime, "state"), "built 1 gone 0");
    let before = runtime.scene().node_count();
    runtime.call_ipc("close", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(
        text(&mut runtime, "state"),
        "built 1 gone 0",
        "still there, leaving"
    );
    assert_eq!(runtime.scene().node_count(), before);
    tick(&mut runtime, 50);
    assert!((number(&mut runtime, "opacity") - 0.5).abs() < 0.01);
    tick(&mut runtime, 60);
    assert_eq!(
        text(&mut runtime, "state"),
        "built 1 gone 1",
        "gone once its exit ended, with its hook"
    );
    assert_eq!(runtime.scene().node_count(), before - 1);
    tick(&mut runtime, 16);
    runtime.poll_services();
    assert_eq!(text(&mut runtime, "state"), "built 1 gone 1", "once");
}

#[test]
fn a_loader_asked_for_again_takes_its_leaving_item_back() {
    let mut runtime = Runtime::default();
    runtime.execute("loader.lua", LOADER).unwrap();
    runtime.poll_services();
    runtime.call_ipc("close", &[]).unwrap();
    runtime.poll_services();
    tick(&mut runtime, 60);
    let midway = number(&mut runtime, "opacity");
    assert!((midway - 0.4).abs() < 0.01, "{midway}");
    runtime.call_ipc("open", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(
        text(&mut runtime, "state"),
        "built 1 gone 0",
        "the same item, not a new one"
    );
    tick(&mut runtime, 16);
    let after = number(&mut runtime, "opacity");
    assert!(after > midway && after < 1.0, "animating back: {after}");
    tick(&mut runtime, 200);
    assert_eq!(number(&mut runtime, "opacity"), 1.0);
    assert_eq!(text(&mut runtime, "state"), "built 1 gone 0");
    let loader = runtime
        .scene()
        .children(runtime.scene().roots()[0])
        .unwrap()[0];
    let page = runtime.scene().children(loader).unwrap()[0];
    assert!(!runtime.scene().is_exiting(page));
    // And let go again, it leaves again.
    runtime.call_ipc("close", &[]).unwrap();
    runtime.poll_services();
    tick(&mut runtime, 120);
    assert_eq!(text(&mut runtime, "state"), "built 1 gone 1");
}

const ROWS: &[u8] = br#"
    local morf = require("morf")
    local ui = require("morf.ui")
    _G.built, _G.gone = 0, 0
    local model = morf.list_model({ "a", "b", "c" })
    _G.nodes = {}
    ui.Repeater {
        as = "column",
        model = model,
        delegate = function(row)
            _G.built = _G.built + 1
            local node = ui.Rect {
                width = 40, height = 20,
                exit = { opacity = 0, duration = 100 },
                on_destroyed = function() _G.gone = _G.gone + 1 end,
            }
            _G.nodes[row] = node
            return node
        end,
    }
    morf.ipc.drop = function() model:remove(2) end
    morf.ipc.put_back = function() model:insert(2, "b") end
    morf.ipc.state = function()
        return ("built %d gone %d"):format(_G.built, _G.gone)
    end
    morf.ipc.opacity = function() return _G.nodes.b.opacity end
"#;

#[test]
fn a_removed_row_leaves_and_a_row_put_back_takes_its_node_back() {
    let mut runtime = Runtime::default();
    runtime.execute("rows.lua", ROWS).unwrap();
    runtime.poll_services();
    assert_eq!(text(&mut runtime, "state"), "built 3 gone 0");
    runtime.call_ipc("drop", &[]).unwrap();
    runtime.poll_services();
    let exiting = runtime.scene().exiting_nodes().count();
    assert_eq!(exiting, 1, "the row's node plays its exit");
    tick(&mut runtime, 50);
    runtime.call_ipc("put_back", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(
        text(&mut runtime, "state"),
        "built 3 gone 0",
        "the node that was leaving, taken back"
    );
    assert_eq!(runtime.scene().exiting_nodes().count(), 0);
    tick(&mut runtime, 200);
    assert_eq!(number(&mut runtime, "opacity"), 1.0);

    runtime.call_ipc("drop", &[]).unwrap();
    runtime.poll_services();
    tick(&mut runtime, 120);
    runtime.poll_services();
    assert_eq!(text(&mut runtime, "state"), "built 3 gone 1");
    runtime.call_ipc("put_back", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(
        text(&mut runtime, "state"),
        "built 4 gone 1",
        "once it has gone, a new row is a new node"
    );
}

#[test]
fn destroy_plays_the_exit_unless_asked_not_to() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "destroy.lua",
            br#"
                local ui = require("morf.ui")
                _G.gone = 0
                _G.slow = ui.Rect {
                    exit = { opacity = 0, duration = 100 },
                    on_destroyed = function() _G.gone = _G.gone + 1 end,
                }
                _G.fast = ui.Rect {
                    exit = { opacity = 0, duration = 100 },
                    on_destroyed = function() _G.gone = _G.gone + 10 end,
                }
                ui.destroy(_G.slow)
                ui.destroy(_G.fast, true)
                morf.ipc.gone = function() return _G.gone end
            "#,
        )
        .unwrap();
    assert_eq!(number(&mut runtime, "gone"), 10.0);
    assert_eq!(runtime.scene().roots().len(), 1);
    tick(&mut runtime, 120);
    assert_eq!(number(&mut runtime, "gone"), 11.0);
    assert!(runtime.scene().roots().is_empty());
}

#[test]
fn a_retainable_lock_outlasts_the_exit() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "retained.lua",
            br#"
                local morf = require("morf")
                local core = require("morf.core")
                local ui = require("morf.ui")
                local active = morf.signal("active", true)
                _G.gone = 0
                ui.Loader {
                    active = function() return active:get() end,
                    source = function()
                        local item = ui.Rect {
                            exit = { opacity = 0, duration = 50 },
                            on_destroyed = function() _G.gone = _G.gone + 1 end,
                        }
                        _G.retained = core.retainable(item, {})
                        _G.lock = core.retain_lock(_G.retained, true)
                        return item
                    end,
                }
                morf.ipc.close = function() active:set(false) end
                morf.ipc.release = function() _G.lock:set_locked(false) end
                morf.ipc.gone = function() return _G.gone end
            "#,
        )
        .unwrap();
    runtime.poll_services();
    runtime.call_ipc("close", &[]).unwrap();
    runtime.poll_services();
    tick(&mut runtime, 80);
    runtime.poll_services();
    assert_eq!(number(&mut runtime, "gone"), 0.0, "still locked");
    runtime.call_ipc("release", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(number(&mut runtime, "gone"), 1.0, "gone with the lock");
}

#[test]
fn enter_may_time_itself_and_exit_is_checked_where_it_is_written() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "enter.lua",
            br#"
                local ui = require("morf.ui")
                _G.card = ui.Rect {
                    opacity = 1,
                    enter = { opacity = 0, duration = 100, easing = "linear" },
                }
                morf.ipc.opacity = function() return _G.card.opacity end
            "#,
        )
        .unwrap();
    assert_eq!(number(&mut runtime, "opacity"), 0.0);
    tick(&mut runtime, 50);
    assert!((number(&mut runtime, "opacity") - 0.5).abs() < 0.01);
    for (source, expected) in [
        ("ui.Rect { exit = { opacityy = 0 } }", "opacityy"),
        (
            "ui.Rect { exit = { opacity = 0, duration = -1 } }",
            "negative",
        ),
        ("ui.Rect { exit = 3 }", "exit must be"),
        (
            "ui.Rect { enter = { opacity = 0, easing = 'linear' } }",
            "duration",
        ),
    ] {
        let error = runtime
            .execute(
                "bad.lua",
                format!("local ui = require('morf.ui')\n{source}").as_bytes(),
            )
            .unwrap_err();
        assert!(error.to_string().contains(expected), "{source}: {error}");
    }
}
