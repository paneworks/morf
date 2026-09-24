use std::time::Duration;

use morf_scene::{Color, ColorSpace, Gradient, GradientKind};

use crate::*;

fn ipc_string(runtime: &mut Runtime, verb: &str) -> String {
    match runtime.call_ipc(verb, &[]).unwrap().as_slice() {
        [IpcValue::String(value)] => value.clone(),
        [IpcValue::Integer(value)] => value.to_string(),
        other => panic!("{verb} answered {other:?}"),
    }
}

#[test]
fn a_gradient_is_one_property_holding_its_stops() {
    // Written as one table of stops, read back with every default filled in,
    // and moved stop by stop when the property has a behavior.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "gradient.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                local accent = morf.color "#3366cc"
                local box = ui.Rect {
                    width = 100,
                    height = 40,
                    gradient = {
                        angle = 90,
                        space = "oklch",
                        stops = { accent, { "#ffffff", 0.25 }, "transparent" },
                    },
                    behavior = { gradient = { duration = 100, easing = "linear", space = "srgb" } },
                }
                morf.ipc.count = function() return #box.gradient.stops end
                morf.ipc.second = function()
                    local stop = box.gradient.stops[2]
                    return tostring(stop.color) .. "@" .. stop.position
                end
                morf.ipc.kind = function() return box.gradient.kind end
                morf.ipc.retarget = function()
                    box.gradient = {
                        angle = 90,
                        space = "oklch",
                        stops = { "#000000", { "#000000", 0.75 }, "transparent" },
                    }
                end
            "##,
        )
        .unwrap();
    let node = runtime.scene().roots()[0];
    let read = |runtime: &Runtime| {
        Gradient::parse(runtime.scene().current(node, "gradient").unwrap())
            .unwrap()
            .unwrap()
    };
    let gradient = read(&runtime);
    assert_eq!(gradient.kind, GradientKind::Linear);
    assert_eq!(gradient.angle, 90.0);
    assert_eq!(gradient.space, ColorSpace::Oklch);
    let positions: Vec<f64> = gradient.stops.iter().map(|stop| stop.position).collect();
    assert_eq!(positions, vec![0.0, 0.25, 1.0]);
    assert_eq!(gradient.stops[0].color, Color::parse("#3366cc").unwrap());
    assert_eq!(gradient.stops[2].color.alpha, 0.0);
    assert_eq!(ipc_string(&mut runtime, "count"), "3");
    assert_eq!(ipc_string(&mut runtime, "second"), "#ffffff@0.25");
    assert_eq!(ipc_string(&mut runtime, "kind"), "linear");

    runtime.call_ipc("retarget", &[]).unwrap();
    runtime.tick_animations(Duration::from_millis(50)).unwrap();
    let halfway = read(&runtime);
    assert_eq!(halfway.stops[1].position, 0.5);
    assert!(
        (halfway.stops[1].color.red - 0.5).abs() < 0.01,
        "the middle stop is halfway from white to black: {:?}",
        halfway.stops[1].color
    );
}

#[test]
fn a_bad_gradient_is_an_error_where_it_is_written() {
    let mut runtime = Runtime::default();
    let error = runtime
        .execute(
            "bad.lua",
            br##"
                local ui = require("morf.ui")
                ui.Rect { gradient = { stops = { "#ffffff" } } }
            "##,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("a gradient needs at least two stops"),
        "{error}"
    );
    let error = runtime
        .execute(
            "bad.lua",
            br##"
                local ui = require("morf.ui")
                ui.Sdf { gradient = { kind = "swirl", stops = { "#fff", "#000" } } }
            "##,
        )
        .unwrap_err();
    assert!(error.to_string().contains("swirl"), "{error}");
}

#[test]
fn a_bound_gradient_retargeted_midway_lands_on_its_last_target() {
    // A game tile whose colour changes again before its last change finished
    // must end on the colour it was last given, not somewhere between.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "tiles.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                local value = morf.signal("value", 0)
                local colours = { "#202020", "#eee4da", "#f2b179", "#f65e3b", "#edc22e" }
                ui.Rect {
                    width = 40,
                    height = 40,
                    gradient = function()
                        local colour = colours[value:get() + 1]
                        return { angle = 135, stops = { colour, { colour, 0.6 }, "#000000" } }
                    end,
                    behavior = { gradient = { duration = 120, easing = "out_cubic" } },
                }
                morf.ipc.set = function(next) value:set(next) end
            "##,
        )
        .unwrap();
    let node = runtime.scene().roots()[0];
    for step in 1..=4 {
        runtime.call_ipc("set", &[IpcValue::Integer(step)]).unwrap();
        runtime.tick_animations(Duration::from_millis(30)).unwrap();
    }
    for _ in 0..20 {
        runtime.tick_animations(Duration::from_millis(16)).unwrap();
    }
    let settled = Gradient::parse(runtime.scene().current(node, "gradient").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(settled.stops[0].color, Color::parse("#edc22e").unwrap());
    assert_eq!(settled.stops[1].color, Color::parse("#edc22e").unwrap());
    assert_eq!(settled.stops[1].position, 0.6);
}

#[test]
fn busy_gradient_tiles_each_land_on_their_last_colour() {
    // Sixteen tiles retargeted at random moments, each swelling on a change
    // through an animation group as a 2048 board does: every one ends on the
    // colour its value names now, not one it was passing through.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "tiles.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                local function mix(a, b, t)
                  a, b = morf.color(a), morf.color(b)
                  return morf.color.rgb(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t,
                    a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
                end
                local function lit(c, top, middle, bottom, at)
                  return { angle = 180, stops = {
                      { mix(c, "#ffffff", top), 0 },
                      { mix(c, "#ffffff", middle), at or 0.5 },
                      { mix(c, "#000000", bottom), 1 } } }
                end
                local colours = { "#202020", "#eee4da", "#f2b179", "#f65e3b", "#edc22e", "#3c3a32" }
                local values, tiles = {}, {}
                for i = 1, 16 do
                  values[i] = morf.signal("v" .. i, 0)
                  local held = values[i]
                  tiles[i] = ui.Rect {
                    width = 40, height = 40,
                    color = "#00000000",
                    gradient = function()
                      local v = held:get()
                      local c = morf.color(colours[v + 1])
                      if v == 0 then return lit(c, 0, 0, 0) end
                      return lit(c, 0.16, 0.03, 0.14, 0.45)
                    end,
                    behavior = { gradient = { duration = 140, easing = { x1 = 0.33, y1 = 1, x2 = 0.68, y2 = 1 } } },
                  }
                end
                ui.Item { table.unpack(tiles) }
                morf.ipc.expected = function(i)
                  local v = values[i]:get()
                  local c = morf.color(colours[v + 1])
                  local g = v == 0 and lit(c, 0, 0, 0) or lit(c, 0.16, 0.03, 0.14, 0.45)
                  return tostring(g.stops[3][1])
                end
                morf.ipc.set = function(i, v)
                  if values[i]:get() ~= v then
                    values[i]:set(v)
                    morf.animation.play { { node = tiles[i], property = "scale", from = 1.16, to = 1, duration = 170, easing = "out_back" } }
                  end
                end
            "##,
        )
        .unwrap();
    let mut seed = 12345u64;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as i64
    };
    for _ in 0..120 {
        for _ in 0..(next() % 5) {
            let i = next() % 16 + 1;
            let v = next() % 6;
            runtime
                .call_ipc("set", &[IpcValue::Integer(i), IpcValue::Integer(v)])
                .unwrap();
        }
        runtime
            .tick_animations(Duration::from_millis((next() % 40) as u64))
            .unwrap();
    }
    for _ in 0..40 {
        runtime.tick_animations(Duration::from_millis(16)).unwrap();
    }
    let root = *runtime.scene().roots().last().unwrap();
    let tiles = runtime.scene().children(root).unwrap().to_vec();
    for (index, tile) in tiles.into_iter().enumerate() {
        let expected = match runtime
            .call_ipc("expected", &[IpcValue::Integer(index as i64 + 1)])
            .unwrap()
            .as_slice()
        {
            [IpcValue::String(value)] => Color::parse(value).unwrap(),
            other => panic!("{other:?}"),
        };
        let scene = runtime.scene();
        assert_eq!(
            scene.current(tile, "gradient").unwrap(),
            scene.target(tile, "gradient").unwrap()
        );
        let got = Gradient::parse(scene.current(tile, "gradient").unwrap())
            .unwrap()
            .unwrap()
            .stops[2]
            .color;
        assert!(
            (got.red - expected.red).abs() < 0.01
                && (got.green - expected.green).abs() < 0.01
                && (got.blue - expected.blue).abs() < 0.01,
            "tile {index}: {got:?} vs {expected:?}"
        );
    }
}
