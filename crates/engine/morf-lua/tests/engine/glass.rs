//! The glass material from Lua: the `glass` table sets the properties it
//! names, a binding reading one of them follows the table, a behavior moves
//! them, and a field glass does not take is refused where it is written.

use std::time::Duration;

use morf_lua::*;

fn answer(runtime: &mut Runtime, verb: &str) -> String {
    match runtime.call_ipc(verb, &[]).unwrap().as_slice() {
        [IpcValue::String(value)] => value.clone(),
        [IpcValue::Number(value)] => value.to_string(),
        [IpcValue::Integer(value)] => value.to_string(),
        other => panic!("{verb} answered {other:?}"),
    }
}

#[test]
fn the_glass_table_sets_its_properties_and_bindings_follow_it() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "glass.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                local pane = ui.Rect {
                    width = 100, height = 60,
                    glass = { refraction = 8, frost = "auto", rim = 0.5, blur = 12, tint = "#00000080" },
                    behavior = { glass_refraction = { duration = 100, easing = "linear" } },
                }
                local seen = ui.Text { text = function() return tostring(pane.glass_frost) end }
                ui.Item { pane, seen }
                morf.ipc.seen = function() return seen.text end
                morf.ipc.material = function()
                    return ("%s %s %s %s"):format(
                        pane.glass_refraction, pane.glass_rim, pane.backdrop_blur, pane.backdrop_tint)
                end
                morf.ipc.clear = function() pane.glass = { frost = 0, refraction = 16 } end
            "##,
        )
        .unwrap();
    assert_eq!(answer(&mut runtime, "material"), "8.0 0.5 12.0 #00000080");
    assert_eq!(answer(&mut runtime, "seen"), "auto");
    runtime.call_ipc("clear", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(answer(&mut runtime, "seen"), "0.0", "the binding followed");
    // The behavior on one of the properties carries the table's change.
    runtime.tick_animations(Duration::from_millis(50)).unwrap();
    let halfway: f64 = answer(&mut runtime, "material")
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!((halfway - 12.0).abs() < 1.0, "half way to 16: {halfway}");
}

#[test]
fn glass_refuses_what_it_does_not_take() {
    let mut runtime = Runtime::default();
    let error = runtime
        .execute(
            "shiny.lua",
            br#"require("morf.ui").Rect { glass = { shine = 1 } }"#,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("`shine` is not part of glass"), "{error}");
    let error = runtime
        .execute(
            "busy.lua",
            br#"require("morf.ui").Rect { glass_frost = "busy" }"#,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("\"auto\""), "{error}");
}
