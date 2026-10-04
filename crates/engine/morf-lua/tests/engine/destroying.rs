//! `on_destroyed`: the one moment a node's own Lua can let go of what it
//! made for the node — subscriptions, timers, a watcher on something else.

use super::*;

#[test]
fn a_repeater_row_that_goes_runs_its_hooks_deepest_first() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "rows.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                _G.seen = {}
                local model = morf.list_model({ "a", "b" })
                ui.Repeater {
                    model = model,
                    delegate = function(row)
                        return ui.Item {
                            on_destroyed = function() table.insert(_G.seen, row) end,
                            ui.Text {
                                text = row,
                                on_destroyed = function() table.insert(_G.seen, row .. ".label") end,
                            },
                        }
                    end,
                }
                morf.ipc.drop = function() model:remove(1) end
                morf.ipc.seen = function() return table.concat(_G.seen, ",") end
            "#,
        )
        .unwrap();
    assert_eq!(
        runtime.call_ipc("seen", &[]).unwrap(),
        [IpcValue::String(String::new())],
        "nothing is destroyed by being built"
    );
    runtime.call_ipc("drop", &[]).unwrap();
    // A view follows its model once a frame, in the poll.
    runtime.poll_services();
    assert_eq!(
        runtime.call_ipc("seen", &[]).unwrap(),
        [IpcValue::String("a.label,a".to_owned())],
        "the row and what was inside it, the child first, once each"
    );
    runtime.call_ipc("drop", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(
        runtime.call_ipc("seen", &[]).unwrap(),
        [IpcValue::String("a.label,a,b.label,b".to_owned())]
    );
}

#[test]
fn a_loader_let_go_runs_the_hooks_of_what_it_loaded() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "loader.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local active = morf.signal("active", true)
                -- The hook releases what the page subscribed to: here, a
                -- signal it writes, which a binding elsewhere follows.
                _G.released = morf.signal("released", 0)
                ui.Item {
                    ui.Loader {
                        active = function() return active:get() end,
                        source = function()
                            return ui.Text {
                                text = "page",
                                on_destroyed = function() _G.released:set(_G.released:get() + 1) end,
                            }
                        end,
                    },
                }
                _G.status = ui.Text { text = function() return "released " .. _G.released:get() end }
                morf.ipc.close = function() active:set(false) end
                morf.ipc.status = function() return _G.status.text end
            "#,
        )
        .unwrap();
    runtime.poll_services();
    runtime.call_ipc("close", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(
        runtime.call_ipc("status", &[]).unwrap(),
        [IpcValue::String("released 1".to_owned())],
        "the hook ran and its write reached the binding"
    );
    runtime.poll_services();
    assert_eq!(
        runtime.call_ipc("status", &[]).unwrap(),
        [IpcValue::String("released 1".to_owned())],
        "once"
    );
}

#[test]
fn destroying_a_node_runs_its_hooks_and_a_failing_hook_is_only_logged() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "destroy.lua",
            br#"
                local ui = require("morf.ui")
                _G.count = 0
                _G.card = ui.Rect {
                    on_destroyed = function() _G.count = _G.count + 1 end,
                    ui.Item { on_destroyed = function() error("the child's hook fails") end },
                }
                ui.destroy(_G.card)
                assert(_G.count == 1, "ran once, at the removal, count " .. _G.count)
            "#,
        )
        .unwrap();
    assert!(runtime.scene().roots().is_empty(), "the node is gone");
    let logs = runtime.take_logs();
    assert!(
        logs.iter()
            .any(|log| log.message.contains("on_destroyed") && log.message.contains("fails")),
        "{logs:?}"
    );
    let error = runtime
        .execute("bad.lua", b"require('morf.ui').Item { on_destroyed = 3 }")
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("on_destroyed must be a function")
    );
}

#[test]
fn a_node_removed_inside_a_binding_runs_its_hook_after_the_flush() {
    // An effect that removes a node does so while the graph is being drained.
    // The hook cannot run there — it may write the very signals the flush is
    // settling — so it runs when the flush is done, and what it writes is
    // flushed in turn.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "flush.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local keep = morf.signal("keep", true)
                _G.gone = morf.signal("gone", "")
                local card = ui.Item { on_destroyed = function() _G.gone:set("card") end }
                _G.label = ui.Text { text = function() return "gone: " .. _G.gone:get() end }
                morf.effect("drop", function()
                    if not keep:get() and card then
                        ui.destroy(card)
                        card = nil
                    end
                end)
                morf.ipc.drop = function() keep:set(false) end
                morf.ipc.label = function() return _G.label.text end
            "#,
        )
        .unwrap();
    runtime.call_ipc("drop", &[]).unwrap();
    assert_eq!(
        runtime.call_ipc("label", &[]).unwrap(),
        [IpcValue::String("gone: card".to_owned())]
    );
}
