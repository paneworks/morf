use morf_scene::{Color, TextDecoration, Value};

use crate::*;

#[test]
fn text_takes_a_line_height_a_slant_a_width_and_a_decoration() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "text.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                local refused = morf.signal("refused", false)
                local label = ui.Text {
                    text = "Password",
                    line_height = "24px",
                    letter_spacing = 1.5,
                    word_spacing = 2,
                    font_style = "italic",
                    font_stretch = "semi_condensed",
                    decoration = function()
                        return refused:get() and { line = "under", color = "#ff0000", thickness = 2 } or {}
                    end,
                }
                morf.ipc.refuse = function() refused:set(true) end
                morf.ipc.line = function() return label.decoration.line end
            "##,
        )
        .unwrap();
    let node = runtime.scene().roots()[0];
    assert_eq!(
        runtime.scene().current(node, "line_height").unwrap(),
        &Value::String("24px".to_owned())
    );
    assert_eq!(
        runtime.scene().string_value(node, "font_style").unwrap(),
        "italic"
    );
    assert_eq!(
        TextDecoration::parse(runtime.scene().current(node, "decoration").unwrap()).unwrap(),
        None,
        "nothing refused yet"
    );
    runtime.call_ipc("refuse", &[]).unwrap();
    let logs: Vec<String> = runtime
        .take_logs()
        .into_iter()
        .map(|entry| format!("{entry:?}"))
        .collect();
    assert!(logs.is_empty(), "{logs:?}");
    let decoration = TextDecoration::parse(runtime.scene().current(node, "decoration").unwrap())
        .unwrap()
        .expect("refused, so underlined");
    assert_eq!(decoration.color, Some(Color::rgba8(255, 0, 0, 255)));
    assert_eq!(decoration.thickness, Some(2.0));
    let line = runtime.call_ipc("line", &[]).unwrap();
    assert_eq!(line, vec![IpcValue::String("under".to_owned())]);
}

#[test]
fn a_wrong_text_style_is_refused_where_it_is_written() {
    let mut runtime = Runtime::default();
    for (source, expected) in [
        (
            r#"ui.Text { text = "x", font_style = "slanted" }"#,
            "`slanted` is not normal, italic or oblique",
        ),
        (
            r#"ui.Text { text = "x", line_height = "tall" }"#,
            "a multiple of the font size or a `px` size",
        ),
        (
            r#"ui.Text { text = "x", decoration = { line = "beside" } }"#,
            "decoration line `beside` is not under, over or through",
        ),
        (
            r#"ui.Text { text = "x", font_stretch = "wide" }"#,
            "`wide` is not a width",
        ),
    ] {
        let error = runtime
            .execute(
                "bad.lua",
                format!("local ui = require(\"morf.ui\")\n{source}").as_bytes(),
            )
            .unwrap_err();
        assert!(error.to_string().contains(expected), "{source}: {error}");
    }
}

#[test]
fn text_in_runs_places_its_links_and_a_click_on_one_follows_it() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "links.lua",
            br##"
                local ui = require("morf.ui")
                local heard = {}
                label = ui.Text {
                    x = 10, y = 20, height = 60, vertical_alignment = "center",
                    font_size = 16,
                    markup = "Open <a href='https://morf.dev/?a=1&amp;b=2'>the page</a> now",
                    on_link = function(href) heard[#heard + 1] = href end,
                }
                styled = ui.Text {
                    spans = { "a ", { text = "b", bold = true, color = "#ff0000" },
                              { text = "c", link = "x:1", underline = false } },
                }
                ui.Item { width = 400, height = 200, label, styled }
                morf.ipc.heard = function() return table.concat(heard, " ") end
                assert(not pcall(function() label.links = {} end))
                assert(not pcall(ui.Text, { spans = { { text = "a", colour = "red" } } }))
                assert(not pcall(ui.Text, { spans = 3 }))
            "##,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let label = runtime.scene().children(root).unwrap()[0];
    let mut text = morf_text::TextSystem::new();
    let layout = runtime
        .compute_layout(
            root,
            morf_layout::Size {
                width: 400.0,
                height: 200.0,
            },
            &mut text,
        )
        .unwrap();
    runtime.sync_text_inputs(&layout, &mut text);
    let Value::List(links) = runtime.scene().current(label, "links").unwrap().clone() else {
        panic!("links is a list");
    };
    assert_eq!(links.len(), 1, "{links:?}");
    let Value::Map(link) = &links[0] else {
        panic!("{links:?}")
    };
    let number = |key: &str| match link.get(key) {
        Some(Value::Number(value)) => *value,
        other => panic!("{key}: {other:?}"),
    };
    assert_eq!(
        link.get("href"),
        Some(&Value::String("https://morf.dev/?a=1&b=2".to_owned()))
    );
    // Centred in 60: the line sits below the node's top.
    assert!(number("y") > 10.0, "{link:?}");
    let (x, y) = (
        number("x") + number("width") / 2.0,
        number("y") + number("height") / 2.0,
    );
    // The link answers the pointer; the words around it do not.
    let hit = layout
        .hit_test(&runtime.scene(), 10.0 + x, 20.0 + y)
        .unwrap()
        .expect("the link is hit");
    assert_eq!(hit.node, label);
    assert!(
        layout
            .hit_test(&runtime.scene(), 12.0, 20.0 + y)
            .unwrap()
            .is_none()
    );
    assert!(runtime.dispatch_pointer(
        label,
        UiEvent::Clicked,
        EventPoint::new((10.0 + x, 20.0 + y), (hit.local_x, hit.local_y)),
        (0.0, 0.0),
    ));
    assert_eq!(
        runtime.call_ipc("heard", &[]).unwrap(),
        [IpcValue::String("https://morf.dev/?a=1&b=2".into())]
    );
}

#[test]
fn axes_are_a_map_of_tags_a_behavior_moves_like_any_number() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "axes.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                local on = morf.signal("on", false)
                icon = ui.Text {
                    text = "home",
                    axes = function() return { FILL = on:get() and 1 or 0, wght = 400 } end,
                    behavior = { axes = { duration = 100, easing = "linear" } },
                }
                plain = ui.Text { text = "x", axes = {} }
                morf.ipc.select = function() on:set(true) end
                morf.ipc.fill = function() return icon.axes.FILL end
                assert(not pcall(ui.Text, { axes = { weight = 400 } }))
                assert(not pcall(ui.Text, { axes = { FILL = "full" } }))
                assert(not pcall(ui.Text, { axes = 3 }))
            "##,
        )
        .unwrap();
    let fill = |runtime: &mut Runtime| match runtime.call_ipc("fill", &[]).unwrap()[..] {
        [IpcValue::Number(value)] => value,
        ref other => panic!("{other:?}"),
    };
    assert_eq!(fill(&mut runtime), 0.0);
    runtime.call_ipc("select", &[]).unwrap();
    runtime
        .tick_animations(std::time::Duration::from_millis(50))
        .unwrap();
    assert!(
        (fill(&mut runtime) - 0.5).abs() < 0.01,
        "half way: {}",
        fill(&mut runtime)
    );
    runtime
        .tick_animations(std::time::Duration::from_millis(60))
        .unwrap();
    assert_eq!(fill(&mut runtime), 1.0);
    let plain = runtime.scene().roots()[1];
    assert_eq!(
        runtime.scene().current(plain, "axes").unwrap(),
        &Value::Map(Default::default()),
        "an empty table is an empty map"
    );
}

#[test]
fn optical_sizing_is_auto_or_none_or_a_boolean() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "optical.lua",
            br##"
                local ui = require("morf.ui")
                ui.Text { text = "a" }
                ui.Text { text = "a", optical_sizing = false }
                ui.Text { text = "a", optical_sizing = "auto" }
                assert(not pcall(ui.Text, { optical_sizing = "yes" }))
                assert(not pcall(ui.Text, { optical_sizing = 1 }))
            "##,
        )
        .unwrap();
    let scene = runtime.scene();
    let sizing = |index: usize| {
        scene
            .current(scene.roots()[index], "optical_sizing")
            .unwrap()
    };
    assert_eq!(sizing(0), &Value::String("auto".into()));
    assert_eq!(sizing(1), &Value::String("none".into()));
    assert_eq!(sizing(2), &Value::String("auto".into()));
}

/// Google Sans Flex, which has a `wdth` axis, when this machine has it.
fn wide_face() -> Option<String> {
    let store = "/nix/store/id27jgbl1sdj8mw04yrwx5bgsv4ap2xg-source/assets/google-sans-flex/GoogleSansFlex-VariableFont_GRAD,ROND,opsz,slnt,wdth,wght.ttf";
    std::iter::once(std::path::PathBuf::from(store))
        .chain(morf_text::family_files("Google Sans Flex"))
        .find(|path| {
            morf_text::file_axes(path)
                .iter()
                .any(|axis| &axis.tag == b"wdth")
        })
        .map(|path| path.to_string_lossy().into_owned())
}

#[test]
fn an_axis_that_widens_text_moves_what_is_laid_out_after_it() {
    let Some(face) = wide_face() else {
        eprintln!("no Google Sans Flex here; skipped");
        return;
    };
    let mut runtime = Runtime::default();
    let source = format!(
        r##"
            local morf = require("morf")
            local ui = require("morf.ui")
            local wide = morf.signal("wide", false)
            ui.Row {{
                ui.Text {{
                    text = "Terminal emulator", font_size = 16,
                    font_family = "Google Sans Flex", font_source = {face:?},
                    axes = function() return {{ wdth = wide:get() and 151 or 75 }} end,
                }},
                ui.Rect {{ width = 10, height = 10 }},
            }}
            morf.ipc.widen = function() wide:set(true) end
        "##
    );
    runtime.execute("wdth.lua", source.as_bytes()).unwrap();
    let root = runtime.scene().roots()[0];
    let after = runtime.scene().children(root).unwrap()[1];
    let available = morf_layout::Size {
        width: 800.0,
        height: 100.0,
    };
    let mut text = morf_text::TextSystem::new();
    let mut layout = runtime.compute_layout(root, available, &mut text).unwrap();
    let before = layout.geometry(after).unwrap().x;
    runtime.call_ipc("widen", &[]).unwrap();
    runtime
        .update_layout(&mut layout, root, available, &mut text)
        .unwrap();
    let moved = layout.geometry(after).unwrap().x;
    assert!(moved > before * 1.3, "{before} -> {moved}");
    let whole = runtime.compute_layout(root, available, &mut text).unwrap();
    assert_eq!(moved, whole.geometry(after).unwrap().x, "as a whole pass");
}
