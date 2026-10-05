use std::time::Duration;

use morf_lua::*;

fn ipc_string(runtime: &mut Runtime, verb: &str) -> String {
    match runtime.call_ipc(verb, &[]).unwrap().as_slice() {
        [IpcValue::String(value)] => value.clone(),
        [IpcValue::Color(color)] => color.to_pastel().to_rgb_hex_string(true),
        [IpcValue::Number(value)] => value.to_string(),
        [IpcValue::Integer(value)] => value.to_string(),
        other => panic!("{verb} answered {other:?}"),
    }
}

#[test]
fn a_theme_holds_colours_and_derives_tokens_from_them() {
    // A colour-named string is a colour, a function is derived from the rest,
    // and a binding that reads a derived token follows the tokens it reads.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "theme.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                local theme = morf.theme {
                    accent = "#3366cc",
                    family = "Inter",
                    hover = function(t) return t.accent:alpha(0.5) end,
                    label = function(t) return t.family .. " on " .. tostring(t.accent) end,
                }
                local box = ui.Rect { color = function() return theme.hover end }
                morf.ipc.hover = function() return theme.hover end
                morf.ipc.label = function() return theme.label end
                morf.ipc.family = function() return theme.family end
                morf.ipc.retint = function() theme.accent = "#ff0000" end
            "##,
        )
        .unwrap();
    assert_eq!(ipc_string(&mut runtime, "hover"), "#3366cc80");
    assert_eq!(ipc_string(&mut runtime, "label"), "Inter on #3366cc");
    assert_eq!(ipc_string(&mut runtime, "family"), "Inter");
    let node = runtime.scene().roots()[0];
    let hex = |runtime: &Runtime| {
        runtime
            .scene()
            .color_value(node, "color")
            .unwrap()
            .to_pastel()
            .to_rgb_hex_string(true)
    };
    assert_eq!(hex(&runtime), "#3366cc80");
    runtime.call_ipc("retint", &[]).unwrap();
    assert_eq!(
        hex(&runtime),
        "#ff000080",
        "the binding re-derived hover from the new accent"
    );
}

#[test]
fn a_theme_reads_its_tokens_from_a_json_file_and_follows_rewrites() {
    let directory = std::env::temp_dir().join(format!("morf-theme-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("colors.json");
    std::fs::write(
        &path,
        br##"{"special": {"background": "#101010"}, "colors": {"color1": "#aa0000", "color2": "#00aa00"}, "alpha": "100"}"##,
    )
    .unwrap();
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "wal.lua",
            format!(
                r##"
                local morf = require("morf")
                local theme = morf.theme({{ color1 = "#ffffff", color9 = "#123456" }}, {{ source = "{}" }})
                morf.ipc.one = function() return theme.color1 end
                morf.ipc.two = function() return theme.color2 end
                morf.ipc.nine = function() return theme.color9 end
                morf.ipc.background = function() return theme.background end
                morf.ipc.alpha = function() return theme.alpha end
                "##,
                path.display()
            )
            .as_bytes(),
        )
        .unwrap();
    assert_eq!(
        ipc_string(&mut runtime, "one"),
        "#aa0000",
        "the file wins over the seed"
    );
    assert_eq!(
        ipc_string(&mut runtime, "two"),
        "#00aa00",
        "a nested leaf is a token"
    );
    assert_eq!(
        ipc_string(&mut runtime, "nine"),
        "#123456",
        "the seed fills what the file lacks"
    );
    assert_eq!(ipc_string(&mut runtime, "background"), "#101010");
    assert_eq!(
        ipc_string(&mut runtime, "alpha"),
        "100",
        "a string that is not a colour stays one"
    );

    // Rewritten the way a palette generator does it: a new file moved over.
    let staged = directory.join("colors.json.new");
    std::fs::write(&staged, br##"{"colors": {"color1": "#0000aa"}}"##).unwrap();
    std::fs::rename(&staged, &path).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        runtime.poll_services();
        if ipc_string(&mut runtime, "one") == "#0000aa" || std::time::Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        ipc_string(&mut runtime, "one"),
        "#0000aa",
        "the token followed the rewrite"
    );
    assert_eq!(
        ipc_string(&mut runtime, "two"),
        "#00aa00",
        "an absent key keeps its value"
    );
    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn a_theme_rejects_what_it_cannot_derive_from() {
    let mut runtime = Runtime::default();
    let error = runtime
        .execute(
            "bad.lua",
            br##"
                local morf = require("morf")
                morf.theme({ accent = "#fff" }, { source = 3 })
            "##,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("theme `source` is a path"),
        "{error}"
    );
}

// A theme with a transition eases a colour written to it from the one on
// show, frame by frame, and every reader follows; other tokens, and a
// theme without one, change at once.
#[test]
fn a_theme_with_a_transition_eases_its_colours() {
    let mut runtime = Runtime::default();
    // A shell on show: before the first tick a colour is simply set.
    runtime.tick_animations(Duration::ZERO).unwrap();
    runtime
        .execute(
            "fade.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                theme = morf.theme({ accent = "#000000", name = "a" },
                    { transition = { duration = 200, easing = "linear" } })
                plain = morf.theme({ accent = "#000000" })
                rect = ui.Rect { color = function() return theme.accent end }
                function red() return math.floor(theme.accent.r * 255 + 0.5) end
                theme.accent = "#ff0000"
                theme.name = "b"
                plain.accent = "#ff0000"
                assert(theme.name == "b", "a string token waited")
                assert(plain.accent:hex() == "#ff0000", "a theme without a transition waited")
                assert(red() == 0, "the colour jumped: " .. red())
            "##,
        )
        .unwrap();
    assert!(runtime.has_motion(), "a fade keeps frames coming");
    runtime.tick_animations(Duration::from_millis(100)).unwrap();
    runtime
        .execute(
            "mid.lua",
            br#"assert(red() > 60 and red() < 250, "not part way: " .. red())"#,
        )
        .unwrap();
    let rect = runtime.scene().roots()[0];
    let mid = runtime.scene().color_value(rect, "color").unwrap();
    runtime.tick_animations(Duration::from_millis(150)).unwrap();
    runtime
        .execute(
            "end.lua",
            br##"assert(theme.accent:hex() == "#ff0000", theme.accent:hex())"##,
        )
        .unwrap();
    assert!(!runtime.has_motion(), "a finished fade keeps drawing");
    assert_ne!(
        runtime.scene().color_value(rect, "color").unwrap(),
        mid,
        "the reader did not follow"
    );
    // Written again mid-way, it sets out from where it is.
    runtime
        .execute(
            "again.lua",
            br##"
                theme.accent = "#0000ff"
            "##,
        )
        .unwrap();
    runtime.tick_animations(Duration::from_millis(100)).unwrap();
    runtime
        .execute(
            "again-mid.lua",
            br#"local c = theme.accent assert(c.r > 0.1 and c.b > 0.1, c:hex())"#,
        )
        .unwrap();
}

// A colour written to a theme with a transition asks for the frame that
// starts the fade, from a handler as from anywhere: before, nothing on
// screen had changed yet, so the fade waited for some other frame.
#[test]
fn a_theme_fade_asks_for_its_first_frame() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "fade.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                theme = morf.theme({ accent = "#000000" }, { transition = { duration = 200 } })
                ui.Rect { color = function() return theme.accent end }
                morf.ipc.set = function() theme.accent = "#ff0000" end
            "##,
        )
        .unwrap();
    runtime.poll_services();
    assert!(!runtime.has_pending_work());
    // A shell on show: before the first tick a colour is simply set.
    runtime.tick_frame_animations(Duration::ZERO).unwrap();
    runtime.call_ipc("set", &[]).unwrap();
    assert!(
        runtime.has_pending_work(),
        "the fade waits for someone else's frame"
    );
    // And each frame of it says so, or the loop stops asking for frames
    // after the first: the scene alone knows nothing of a theme fade.
    let frame = runtime.tick_frame_animations(Duration::ZERO).unwrap();
    assert!(frame.active, "a fading theme is not motion to the loop");
    let frame = runtime
        .tick_frame_animations(Duration::from_millis(100))
        .unwrap();
    assert!(frame.active && frame.changed > 0);
    runtime
        .tick_frame_animations(Duration::from_millis(200))
        .unwrap();
    let frame = runtime
        .tick_frame_animations(Duration::from_millis(16))
        .unwrap();
    assert!(!frame.active, "the fade never ends");
}

#[test]
fn a_theme_colour_set_before_anything_is_shown_is_set_at_once() {
    // A scheme read while the configuration loads: nothing has been drawn
    // to ease from, and easing would re-run every binding for frames.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "load.lua",
            br##"
                local morf = require("morf")
                theme = morf.theme({ accent = "#000000" }, { transition = { duration = 400 } })
                theme.accent = "#ff0000"
                assert(theme.accent:hex() == "#ff0000", "it eased: " .. theme.accent:hex())
            "##,
        )
        .unwrap();
    assert!(!runtime.has_motion(), "nothing is fading");
}
