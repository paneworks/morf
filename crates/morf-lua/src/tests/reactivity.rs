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
fn a_signal_holds_a_table_by_value() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "table-signal.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                local seed = { title = "a", tags = { "x", "y" }, count = 2 }
                local window = morf.signal("window", seed)
                seed.title = "changed behind its back"
                local runs = 0
                ui.Text { text = function()
                    runs = runs + 1
                    local w = window:get()
                    return w.title .. ":" .. table.concat(w.tags, ",") .. ":" .. w.count
                end }
                morf.ipc.same = function()
                    window:set({ count = 2, tags = { "x", "y" }, title = "a" })
                end
                morf.ipc.retitle = function()
                    local w = window:get()
                    w.title = "b"
                    window:set(w)
                end
                morf.ipc.mutate_copy = function()
                    window:get().title = "ignored"
                    return window:get().title
                end
                morf.ipc.runs = function() return runs end
                morf.ipc.get = function() return window:get() end
                morf.ipc.bad = function() window:set({ 1, 2, x = 3 }) end
                morf.ipc.cycle = function()
                    local t = {}
                    t.self = t
                    window:set(t)
                end
            "##,
        )
        .unwrap();
    assert_eq!(text(&runtime, 0), "a:x,y:2");
    // An equal table is no change: nothing re-runs.
    runtime.call_ipc("same", &[]).unwrap();
    assert_eq!(
        runtime.call_ipc("runs", &[]).unwrap(),
        [IpcValue::Integer(1)]
    );
    // What `get` hands out is a copy.
    assert_eq!(
        runtime.call_ipc("mutate_copy", &[]).unwrap(),
        [IpcValue::String("a".into())]
    );
    runtime.call_ipc("retitle", &[]).unwrap();
    assert_eq!(text(&runtime, 0), "b:x,y:2");
    let [value] = &runtime.call_ipc("get", &[]).unwrap()[..] else {
        panic!("one value")
    };
    assert_eq!(
        value.to_json(),
        serde_json::json!({ "title": "b", "tags": ["x", "y"], "count": 2 })
    );
    let error = runtime.call_ipc("bad", &[]).unwrap_err().to_string();
    assert!(error.contains("not both"), "{error}");
    let error = runtime.call_ipc("cycle", &[]).unwrap_err().to_string();
    assert!(error.contains("nests deeper"), "{error}");
}

#[test]
fn a_mouse_area_says_whether_it_is_hovered_and_pressed() {
    // A port made a signal per button and wrote it from on_entered and
    // on_exited; the area now keeps both, for bindings to follow.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "hover.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                area = ui.MouseArea { width = 10, height = 10 }
                ui.Text { text = function()
                    return (area.hovered and "h" or "-") .. (area.pressed and "p" or "-")
                end }
                morf.ipc.write = function() area.hovered = true end
            "#,
        )
        .unwrap();
    let area = runtime.scene().roots()[0];
    assert_eq!(text(&runtime, 1), "--");
    assert!(runtime.dispatch_ui_event(area, UiEvent::PointerEntered));
    assert_eq!(text(&runtime, 1), "h-");
    let point = EventPoint::default();
    assert!(runtime.dispatch_pointer(area, UiEvent::Pressed, point, (0.0, 0.0)));
    assert_eq!(text(&runtime, 1), "hp");
    runtime.dispatch_pointer(area, UiEvent::Released, point, (0.0, 0.0));
    runtime.dispatch_ui_event(area, UiEvent::PointerExited);
    assert_eq!(text(&runtime, 1), "--");
    let error = runtime.call_ipc("write", &[]).unwrap_err().to_string();
    assert!(error.contains("read-only"), "{error}");
    let error = Runtime::default()
        .execute(
            "bad.lua",
            br#"require("morf.ui").MouseArea { hovered = true }"#,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("read-only"), "{error}");
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
fn an_effect_made_inside_a_flush_can_be_disposed() {
    // Made inside another effect, an effect is queued until the running
    // flush ends; its handle disposes it all the same, queued or not.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "nested.lua",
            br#"
                local morf = require("morf")
                local tick = morf.signal("tick", 0)
                local runs = 0
                local inner, early
                morf.effect("outer", function()
                    if inner then return end
                    inner = morf.effect("inner", function() tick:get(); runs = runs + 1 end)
                    early = morf.effect("early", function() tick:get(); runs = runs + 100 end)
                    early:dispose()
                end)
                morf.ipc.tick = function() tick:set(tick:get() + 1) end
                morf.ipc.dispose = function() return inner:dispose() end
                morf.ipc.runs = function() return runs end
            "#,
        )
        .unwrap();
    assert_eq!(
        runtime.call_ipc("runs", &[]).unwrap(),
        [IpcValue::Integer(1)]
    );
    runtime.call_ipc("tick", &[]).unwrap();
    assert_eq!(
        runtime.call_ipc("runs", &[]).unwrap(),
        [IpcValue::Integer(2)]
    );
    assert_eq!(
        runtime.call_ipc("dispose", &[]).unwrap(),
        [IpcValue::Boolean(true)]
    );
    runtime.call_ipc("tick", &[]).unwrap();
    assert_eq!(
        runtime.call_ipc("runs", &[]).unwrap(),
        [IpcValue::Integer(2)]
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
