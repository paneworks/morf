//! What bindings follow: list models, effects that end, table signals.

use crate::*;

fn text(runtime: &Runtime, index: usize) -> String {
    let node = runtime.scene().roots()[index];
    runtime
        .scene()
        .string_value(node, "text")
        .unwrap()
        .to_owned()
}

#[test]
fn a_binding_that_reads_a_list_model_follows_it() {
    // A binding computing from a list model used to read it once: the model
    // is not a signal, so nothing told the binding it had changed.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "model.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local model = morf.list_model({ 1, 2 })
                local runs = 0
                ui.Text { text = function() return tostring(model:len()) end }
                ui.Text { text = function()
                    runs = runs + 1
                    local sum = 0
                    for i = 1, model:len() do sum = sum + model:get(i) end
                    return string.format("%d", sum)
                end }
                ui.Text { text = function()
                    local out = {}
                    for _, value in ipairs(model) do out[#out + 1] = string.format("%d", value) end
                    return #model .. ":" .. table.concat(out, ",")
                end }
                morf.ipc.push = function(value) model:insert(model:len() + 1, value) end
                morf.ipc.drop = function() model:remove(1) end
                morf.ipc.same = function() model:set(1, model:get(1)) end
                morf.ipc.swap = function() model:replace({ 10, 20, 30 }) end
                morf.ipc.runs = function() return runs end
                -- Outside a handler a change flushes at once, as a signal does.
                model:set(1, 5)
            "#,
        )
        .unwrap();
    assert_eq!(text(&runtime, 0), "2");
    assert_eq!(text(&runtime, 1), "7");
    assert_eq!(text(&runtime, 2), "2:5,2");

    runtime.call_ipc("push", &[IpcValue::Integer(4)]).unwrap();
    assert_eq!(text(&runtime, 0), "3");
    assert_eq!(text(&runtime, 1), "11");
    assert_eq!(text(&runtime, 2), "3:5,2,4");

    runtime.call_ipc("drop", &[]).unwrap();
    assert_eq!(text(&runtime, 1), "6");

    // Writing a row its own value changes nothing, and runs nothing.
    let before = runtime.call_ipc("runs", &[]).unwrap();
    runtime.call_ipc("same", &[]).unwrap();
    assert_eq!(runtime.call_ipc("runs", &[]).unwrap(), before);

    runtime.call_ipc("swap", &[]).unwrap();
    assert_eq!(text(&runtime, 0), "3");
    assert_eq!(text(&runtime, 1), "60");
}

#[test]
fn a_state_list_is_followed_when_assigned_whole() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "state-list.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local model = morf.state { rows = { "a" } }
                ui.Text { text = function() return tostring(model.rows:len()) end }
                morf.ipc.set = function() model.rows = { "a", "b", "c" } end
            "#,
        )
        .unwrap();
    assert_eq!(text(&runtime, 0), "1");
    runtime.call_ipc("set", &[]).unwrap();
    assert_eq!(text(&runtime, 0), "3");
}

#[test]
fn a_disposed_effect_never_runs_again() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "dispose.lua",
            br#"
                local morf = require("morf")
                local tick = morf.signal("tick", 0)
                local runs = 0
                local effect = assert(morf.effect("counted", function()
                    tick:get()
                    runs = runs + 1
                end))
                morf.ipc.tick = function() tick:set(tick:get() + 1) end
                morf.ipc.dispose = function() return effect:dispose(), effect:alive() end
                morf.ipc.runs = function() return runs end
                -- An effect that fails its first run still hands back its handle.
                local ok, err, failing = morf.effect("failing", function()
                    tick:get()
                    error("nope")
                end)
                assert(ok == false and err:find("nope") and failing:alive())
                failing:dispose()
            "#,
        )
        .unwrap();
    runtime.call_ipc("tick", &[]).unwrap();
    assert_eq!(
        runtime.call_ipc("runs", &[]).unwrap(),
        [IpcValue::Integer(2)]
    );
    let before = runtime.resource_stats();
    assert_eq!(
        runtime.call_ipc("dispose", &[]).unwrap(),
        [IpcValue::Boolean(true), IpcValue::Boolean(false)]
    );
    assert_eq!(
        runtime.resource_stats().graph_effects,
        before.graph_effects - 1
    );
    runtime.call_ipc("tick", &[]).unwrap();
    runtime.call_ipc("tick", &[]).unwrap();
    assert_eq!(
        runtime.call_ipc("runs", &[]).unwrap(),
        [IpcValue::Integer(2)]
    );
    // A second dispose is a no-op.
    assert_eq!(
        runtime.call_ipc("dispose", &[]).unwrap(),
        [IpcValue::Boolean(false), IpcValue::Boolean(false)]
    );
}

#[test]
fn an_owned_effect_goes_with_its_node() {
    // An effect made per delegate -- per panel build -- outlived the node it
    // was made for, and every build added one more to the graph.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "owned.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local tick = morf.signal("tick", 0)
                local runs = 0
                local model = morf.list_model({ "a", "b" })
                ui.Repeater {
                    model = model,
                    delegate = function(row)
                        local node = ui.Item {}
                        assert(morf.effect("row " .. row, function()
                            tick:get()
                            runs = runs + 1
                        end, { owner = node }))
                        return node
                    end,
                }
                morf.ipc.tick = function() tick:set(tick:get() + 1) end
                morf.ipc.drop = function() model:remove(1) end
                morf.ipc.runs = function() return runs end
            "#,
        )
        .unwrap();
    runtime.poll_services();
    let runs = |runtime: &mut Runtime| match runtime.call_ipc("runs", &[]).unwrap()[..] {
        [IpcValue::Integer(runs)] => runs,
        ref other => panic!("{other:?}"),
    };
    let first = runs(&mut runtime);
    runtime.call_ipc("tick", &[]).unwrap();
    assert_eq!(runs(&mut runtime), first + 2);
    let before = runtime.resource_stats();

    runtime.call_ipc("drop", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(
        runtime.resource_stats().graph_effects,
        before.graph_effects - 1
    );
    let settled = runs(&mut runtime);
    runtime.call_ipc("tick", &[]).unwrap();
    assert_eq!(runs(&mut runtime), settled + 1);
}
