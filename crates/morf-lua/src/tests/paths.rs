//! `ui.Path` from Lua: its numbers animate like any other, and its words are
//! checked where they are written.

use std::time::Duration;

use super::*;

#[test]
fn a_path_animates_its_trim_colour_and_morph() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "ring.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                local level = morf.signal("level", 20)
                _G.ring = ui.Path {
                    width = 48, height = 48, view_box = { x = 0, y = 0, w = 100, h = 100 },
                    d = "M50 8 A42 42 0 1 1 49.99 8",
                    morph_to = "M50 20 A30 30 0 1 1 49.99 20",
                    fill_color = "transparent", stroke_color = "#3366cc",
                    stroke_width = 12, stroke_cap = "round", dash = { 4, 2 },
                    trim_end = function() return level:get() / 100 end,
                    behavior = {
                        trim_end = { duration = 100 },
                        stroke_color = { duration = 100 },
                        morph_progress = { duration = 100 },
                        dash = { duration = 100 },
                    },
                }
                morf.ipc.fill = function()
                    level:set(80)
                    _G.ring.stroke_color = "#ff6600"
                    _G.ring.morph_progress = 1
                    _G.ring.dash = { 8, 4 }
                end
            "##,
        )
        .unwrap();
    let ring = runtime.scene().roots()[0];
    assert_eq!(runtime.scene().number(ring, "trim_end").unwrap(), 0.2);
    runtime.call_ipc("fill", &[]).unwrap();
    runtime.tick_animations(Duration::from_millis(50)).unwrap();
    let scene = runtime.scene();
    let trim = scene.number(ring, "trim_end").unwrap();
    assert!(trim > 0.2 && trim < 0.8, "the trim is on its way: {trim}");
    let morph = scene.number(ring, "morph_progress").unwrap();
    assert!(morph > 0.0 && morph < 1.0, "and the outline: {morph}");
    let morf_scene::Value::List(dash) = scene.current(ring, "dash").unwrap() else {
        panic!("dash is a list");
    };
    let morf_scene::Value::Number(first) = dash[0] else {
        panic!("of lengths");
    };
    assert!(first > 4.0 && first < 8.0, "and every dash length: {first}");
    drop(scene);
    runtime.tick_animations(Duration::from_millis(60)).unwrap();
    assert_eq!(runtime.scene().number(ring, "trim_end").unwrap(), 0.8);
    // Its view box is its size when it is given none.
    let sized = runtime.execute(
        "sized.lua",
        br#"
                local ui = require("morf.ui")
                _G.icon = ui.Path { view_box = { 0, 0, 24, 24 }, d = "M0 0 H24 V24 Z" }
            "#,
    );
    assert!(sized.is_ok(), "{sized:?}");
}

#[test]
fn a_path_refuses_what_it_cannot_draw() {
    let mut runtime = Runtime::default();
    for (source, message) in [
        (
            "require('morf.ui').Path { d = 'M0 0 L' }",
            "not SVG path data",
        ),
        (
            "require('morf.ui').Path { d = 'M0 0 L1 1', stroke_join = 'pointy' }",
            "miter, round, bevel",
        ),
        (
            "require('morf.ui').Path { view_box = { x = 0, y = 0, w = 0, h = 4 } }",
            "positive width",
        ),
        (
            "require('morf.ui').Path { dash = { -1, 2 } }",
            "non-negative",
        ),
    ] {
        let error = runtime.execute("bad.lua", source.as_bytes()).unwrap_err();
        assert!(error.to_string().contains(message), "{source}: {error}");
    }
}
