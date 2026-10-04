use morf_scene::Color;

use morf_lua::*;

fn ipc_string(runtime: &mut Runtime, verb: &str) -> String {
    match runtime.call_ipc(verb, &[]).unwrap().as_slice() {
        [IpcValue::String(value)] => value.clone(),
        [IpcValue::Color(color)] => color.to_pastel().to_rgb_hex_string(true),
        other => panic!("{verb} answered {other:?}"),
    }
}

fn ipc_number(runtime: &mut Runtime, verb: &str) -> f64 {
    match runtime.call_ipc(verb, &[]).unwrap().as_slice() {
        [IpcValue::Number(value)] => *value,
        [IpcValue::Integer(value)] => *value as f64,
        other => panic!("{verb} answered {other:?}"),
    }
}

#[test]
fn a_colour_is_a_value_a_property_takes_and_gives_back() {
    // Written as a value, read back as the same value: fields, string
    // form, equality, and the round trip through a signal and a state.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "color.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                local accent = morf.color "#3366cc"
                local tint = morf.signal("tint", accent:alpha(0.5))
                local model = morf.state { paper = morf.color.oklch(0.95, 0.02, 80) }
                local box = ui.Rect {
                    color = accent,
                    border_color = function() return tint:get() end,
                }
                morf.ipc.read = function() return tostring(box.color) end
                morf.ipc.alpha = function() return box.border_color.a end
                morf.ipc.hue = function() return math.floor(box.color.h + 0.5) end
                morf.ipc.paper = function() return model.paper end
                morf.ipc.same = function()
                    return box.color == morf.color.rgb(0x33, 0x66, 0xcc) and "same" or "different"
                end
                morf.ipc.retint = function() tint:set(morf.color "rebeccapurple") end
                morf.ipc.name = function() return accent:nearest_name() end
            "##,
        )
        .unwrap();
    let node = runtime.scene().roots()[0];
    assert_eq!(
        runtime.scene().color_value(node, "color").unwrap(),
        Color::parse("#3366cc").unwrap()
    );
    assert_eq!(ipc_string(&mut runtime, "read"), "#3366cc");
    assert!((ipc_number(&mut runtime, "alpha") - 0.5).abs() < 1e-6);
    assert_eq!(ipc_number(&mut runtime, "hue"), 220.0);
    assert_eq!(ipc_string(&mut runtime, "same"), "same");
    assert_eq!(ipc_string(&mut runtime, "name"), "royalblue");
    let paper = runtime.call_ipc("paper", &[]).unwrap();
    assert!(matches!(paper.as_slice(), [IpcValue::Color(_)]));
    runtime.call_ipc("retint", &[]).unwrap();
    assert_eq!(
        runtime.scene().color_value(node, "border_color").unwrap(),
        Color::parse("rebeccapurple").unwrap()
    );
}

#[test]
fn every_pastel_operation_has_a_lua_counterpart() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "ops.lua",
            br##"
                local morf = require("morf")
                local c = morf.color "hsl(200, 60%, 40%)"
                local answers = {}
                local function put(name, value) answers[#answers + 1] = name .. "=" .. tostring(value) end
                put("hex", c:hex())
                put("rgb", c:rgb_string())
                put("hsl", c:hsl_string())
                put("oklch", c:distance(morf.color(c:oklch_string())) < 1)
                put("lighter", c:lighten(0.2))
                put("rotated", c:rotate(180) == c:complement())
                put("gray", c:gray():hsl().s)
                put("mixed", c:mix("white", 0.5, "srgb"))
                put("over", morf.color("#ff000080"):composite("white"))
                put("blind", c:blind("deuteranopia") ~= c)
                put("with", c:with { l = 0.9, space = "hsl" }:hsl().l)
                put("light", c:is_light())
                put("contrast", string.format("%.2f", c:contrast("white")))
                put("text", c:text_color())
                put("distance", c:distance("white", "cie76") > 0)
                put("scale", morf.color.scale { "black", "white" }:sample(0.5, "srgb"))
                put("samples", #morf.color.scale { "black", "white" }:samples(5))
                put("distinct", #morf.color.distinct(4, { fixed = { "red" }, order = true, iterations = 3000 }))
                put("random", morf.color.random("gray"):hsl().s)
                put("named", morf.color.named("Tomato"))
                put("names", morf.color.names().tomato == morf.color.named("tomato"))
                put("ansi", morf.color.ansi8(9):hex() .. ":" .. c:ansi8())
                put("paint", c:paint("x", { bold = true, mode = "8bit" }) ~= "x")
                put("table", morf.color { r = 255, g = 0, b = 0 })
                put("hwb", morf.color "hwb(0 0% 0%)")
                put("slash", morf.color "rgb(255 0 0 / 50%)")
                put("cmyk", morf.color.cmyk(0, 1, 1, 0))
                put("invert", morf.color("#ff0000"):invert())
                put("gray8", morf.color.gray(0.5):rgb8().r)
                morf.ipc.answers = function() return table.concat(answers, " ") end
            "##,
        )
        .unwrap();
    let answers = ipc_string(&mut runtime, "answers");
    let expect = |pair: &str| assert!(answers.contains(pair), "{answers} lacks {pair}");
    expect("hex=#297aa3");
    expect("rgb=rgb(41, 122, 163)");
    expect("hsl=hsl(200, 59.8%, 40.0%)");
    expect("oklch=true");
    expect("lighter=#5cadd6");
    expect("rotated=true");
    expect("gray=0.0");
    expect("mixed=#94bdd1");
    expect("over=#ff7e7e");
    expect("blind=true");
    expect("with=0.9");
    expect("light=false");
    expect("contrast=4.77");
    expect("text=#ffffff");
    expect("distance=true");
    expect("scale=#808080");
    expect("samples=5");
    expect("distinct=4");
    expect("random=0.0");
    expect("named=#ff6347");
    expect("names=true");
    expect("ansi=#ff0000:");
    expect("paint=true");
    expect("table=#ff0000");
    expect("hwb=#ff0000");
    expect("slash=#ff000080");
    expect("cmyk=#ff0000");
    expect("invert=#00ffff");
    expect("gray8=128");
}

#[test]
fn a_bad_colour_is_an_error_where_it_is_written() {
    let mut runtime = Runtime::default();
    let error = runtime
        .execute(
            "bad.lua",
            br##"
                local morf = require("morf")
                local c = morf.color "#\xc3\xa9\xc3\xa9"
            "##,
        )
        .unwrap_err();
    assert!(error.to_string().contains("is not a colour"), "{error}");
    let error = runtime
        .execute(
            "bad-space.lua",
            br##"
                local morf = require("morf")
                morf.color("red"):mix("blue", 0.5, "cielab")
            "##,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("unknown mixing space"),
        "{error}"
    );
}

#[test]
fn a_colour_behavior_names_its_space_and_direction() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "space.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                ui.Rect {
                    color = "red",
                    behavior = {
                        color = { duration = 1000, space = "oklch", hue = "longer" },
                    },
                }
            "##,
        )
        .unwrap();
    let error = runtime
        .execute(
            "bad-space.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                ui.Rect { color = "red", behavior = { color = { duration = 1, space = "hsl" } } }
            "##,
        )
        .unwrap_err();
    assert!(error.to_string().contains("space must be"), "{error}");
}

#[test]
fn linear_light_is_the_shader_side_of_a_colour() {
    // What a shader is handed: the sRGB curve taken off, so a data block
    // carries the numbers the GPU multiplies rather than the ones a
    // stylesheet writes.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "linear.lua",
            br##"
                local morf = require("morf")
                local linear = morf.color("#808080"):linear()
                morf.ipc.gray = function() return linear.r end
                morf.ipc.alpha = function() return morf.color("#ff000080"):linear().a end
            "##,
        )
        .unwrap();
    assert!((ipc_number(&mut runtime, "gray") - 0.2158).abs() < 0.001);
    assert!((ipc_number(&mut runtime, "alpha") - 0.5).abs() < 0.01);
}

#[test]
fn hct_colours_and_tonal_palettes() {
    // Hue, chroma and tone as Material Color Utilities computes them, and
    // the tones of blue its palette test lists.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "hct.lua",
            br##"
                local morf = require("morf")
                local function near(a, b, d) return math.abs(a - b) <= d end
                local h, c, t = morf.color("#ff0000"):hct()
                assert(near(h, 27.408, 0.001) and near(c, 113.357, 0.001) and near(t, 53.233, 0.01),
                    h .. " " .. c .. " " .. t)
                -- In gamut: back to the same colour.
                assert(morf.color.hct(h, c, t):hex() == "#ff0000")
                -- Out of gamut: tone and hue kept, chroma given up.
                local vivid = morf.color.hct(140, 200, 50)
                local vh, vc, vt = vivid:hct()
                assert(near(vt, 50, 0.5) and near(vh, 140, 4) and vc < 200, vh .. " " .. vc .. " " .. vt)
                assert(morf.color.hct(0, 0, 100):hex() == "#ffffff")
                assert(morf.color.hct(0, 0, 0):hex() == "#000000")
                assert(near(morf.color.hct(10, 20, 30, 0.5).a, 0.5, 0.01))
                -- As a table, and changed by `with`.
                assert(morf.color { h = h, c = c, t = t }:hex() == "#ff0000")
                local _, _, darker = morf.color("#ff0000"):with({ t = 30 }):hct()
                assert(near(darker, 30, 0.5), darker)

                local blue = morf.color.tonal_palette(morf.color "#0000ff")
                local expected = {
                    [100] = "#ffffff", [95] = "#f1efff", [90] = "#e0e0ff", [80] = "#bec2ff",
                    [70] = "#9da3ff", [60] = "#7c84ff", [50] = "#5a64ff", [40] = "#343dff",
                    [30] = "#0000ef", [20] = "#0001ac", [10] = "#00006e", [0] = "#000000",
                }
                for tone, hex in pairs(expected) do
                    assert(blue(tone):hex() == hex, tone .. ": " .. blue(tone):hex())
                    assert(blue[tone]:hex() == hex)
                    assert(blue:tone(tone):hex() == hex)
                end
                assert(near(blue.hue, 282.788, 0.001) and near(blue.chroma, 87.230, 0.001))
                local neutral = morf.color.tonal_palette(270, 4)
                local _, nc, nt = neutral(50):hct()
                assert(near(nc, 4, 1) and near(nt, 50, 0.5))
                assert(not pcall(morf.color.hct, "x", 1, 2))
            "##,
        )
        .unwrap();
}
