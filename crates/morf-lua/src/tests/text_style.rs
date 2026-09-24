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
