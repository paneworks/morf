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
