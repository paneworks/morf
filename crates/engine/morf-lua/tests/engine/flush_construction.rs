use morf_lua::*;

fn answer(runtime: &mut Runtime, verb: &str) -> IpcValue {
    runtime.call_ipc(verb, &[]).unwrap().remove(0)
}

#[test]
fn an_effect_that_builds_nodes_with_bindings_does_not_panic() {
    // Building a node with a binding inside `morf.effect` registered the
    // binding while the flush held the graph, and the `.expect` there took
    // the whole engine down. The binding is now queued and runs when the
    // flush ends.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "effect_builds.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local name = morf.signal("name", "a")
                local count = morf.signal("count", 1)
                local label
                morf.effect("build", function()
                    count:get()
                    local text = ui.Text { text = function() return name:get() .. "!" end }
                    ui.Row { text }
                    label = text
                end)
                morf.ipc.label = function() return label.text end
                morf.ipc.rename = function() name:set("b") end
                morf.ipc.rebuild = function() count:set(count:get() + 1) end
            "#,
        )
        .unwrap();
    assert_eq!(answer(&mut runtime, "label"), IpcValue::String("a!".into()));

    runtime.call_ipc("rename", &[]).unwrap();
    assert_eq!(answer(&mut runtime, "label"), IpcValue::String("b!".into()));

    // The effect running again builds a fresh tree, whose binding is live.
    runtime.call_ipc("rebuild", &[]).unwrap();
    assert_eq!(answer(&mut runtime, "label"), IpcValue::String("b!".into()));
}

#[test]
fn a_binding_that_constructs_nodes_does_not_panic() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "binding_builds.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local word = morf.signal("word", "x")
                local made
                local outer = ui.Text {
                    text = function()
                        local w = word:get()
                        made = ui.Text { text = function() return w .. w end }
                        return w
                    end,
                }
                morf.ipc.outer = function() return outer.text end
                morf.ipc.made = function() return made.text end
                morf.ipc.set = function() word:set("y") end
            "#,
        )
        .unwrap();
    assert_eq!(answer(&mut runtime, "outer"), IpcValue::String("x".into()));
    assert_eq!(answer(&mut runtime, "made"), IpcValue::String("xx".into()));
    runtime.call_ipc("set", &[]).unwrap();
    assert_eq!(answer(&mut runtime, "outer"), IpcValue::String("y".into()));
    assert_eq!(answer(&mut runtime, "made"), IpcValue::String("yy".into()));
}

#[test]
fn an_effect_made_inside_an_effect_runs_after_it() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "nested_effect.lua",
            br#"
                local morf = require("morf")
                local source = morf.signal("source", 1)
                local seen = 0
                local made = false
                morf.effect("outer", function()
                    if made then return end
                    made = true
                    morf.effect("inner", function() seen = source:get() end)
                end)
                morf.ipc.seen = function() return seen end
                morf.ipc.bump = function() source:set(7) end
            "#,
        )
        .unwrap();
    assert_eq!(answer(&mut runtime, "seen"), IpcValue::Integer(1));
    runtime.call_ipc("bump", &[]).unwrap();
    assert_eq!(answer(&mut runtime, "seen"), IpcValue::Integer(7));
}
