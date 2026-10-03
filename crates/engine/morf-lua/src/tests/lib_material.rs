//! `library/lib/material.lua`: Material 3 schemes over `morf.color`'s HCT,
//! and the source colour of a picture.

use std::path::Path;
use std::time::{Duration, Instant};

use morf_image::ops::{OutputFormat, save_rgba};

use super::*;

fn runtime(source: &str) -> Runtime {
    let mut runtime = Runtime::default();
    runtime.set_module_roots(vec![
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../library"),
    ]);
    let prelude = r#"
        local material = require("lib.material")
        results = {}
        morf.ipc.result = function(key) return results[key] end
    "#;
    runtime
        .execute("material.lua", format!("{prelude}\n{source}").as_bytes())
        .unwrap();
    runtime
}

fn result(runtime: &mut Runtime, key: &str) -> Option<String> {
    let value = runtime
        .call_ipc("result", &[IpcValue::String(key.to_owned())])
        .unwrap();
    match value.as_slice() {
        [IpcValue::String(text)] => Some(text.clone()),
        _ => None,
    }
}

#[test]
fn a_scheme_places_every_role_at_its_tone() {
    let mut runtime = runtime(
        r##"
        local out = {}
        for _, mode in ipairs { "dark", "light" } do
            for _, variant in ipairs(material.VARIANTS) do
                local s = material.scheme("#4a7fb5", { mode = mode, variant = variant })
                for _, role in ipairs(material.ROLES) do assert(s[role], role) end
            end
        end
        local dark = material.scheme("#4a7fb5", { mode = "dark" })
        local light = material.scheme("#4a7fb5", { mode = "light" })
        local function tone(c) local _, _, t = c:hct() return string.format("%.0f", t) end
        results.tones = table.concat({
            tone(dark.primary), tone(dark.onPrimary), tone(dark.surface),
            tone(light.primary), tone(light.onPrimary), tone(light.surface),
        }, ",")
        -- The source's hue is the primary's in tonal spot; the tertiary is
        -- sixty degrees on.
        local sh = morf.color("#4a7fb5"):hct()
        local ph = dark.primary:hct()
        local th = dark.tertiary:hct()
        results.hues = string.format("%.0f,%.0f", (ph - sh + 360) % 360, (th - sh + 360) % 360)
        -- Monochrome has no colour (HCT reads a grey as chroma ~2).
        local _, c = material.scheme("#ff0000", { variant = "monochrome" }).primary:hct()
        results.mono = c < 3 and "grey" or tostring(c)
        -- Every "on" colour reads on what it sits on.
        local worst = math.huge
        for _, s in ipairs { dark, light } do
            for _, pair in ipairs {
                { "onPrimary", "primary" }, { "onPrimaryContainer", "primaryContainer" },
                { "onSecondaryContainer", "secondaryContainer" }, { "onSurface", "surface" },
                { "onSurfaceVariant", "surfaceVariant" }, { "onError", "error" },
            } do
                worst = math.min(worst, s[pair[1]]:contrast(s[pair[2]]))
            end
        end
        results.contrast = string.format("%.1f", worst)
        results.hex = material.hex(dark).primary
    "##,
    );
    assert_eq!(result(&mut runtime, "tones").unwrap(), "80,20,6,40,100,98");
    let hues = result(&mut runtime, "hues").unwrap();
    let (primary, tertiary) = hues.split_once(',').unwrap();
    let near = |value: &str, want: f64| {
        let value: f64 = value.parse().unwrap();
        let d = (value - want).rem_euclid(360.0);
        d.min(360.0 - d) <= 2.0
    };
    assert!(near(primary, 0.0) && near(tertiary, 60.0), "{hues}");
    assert_eq!(result(&mut runtime, "mono").unwrap(), "grey");
    let contrast: f64 = result(&mut runtime, "contrast").unwrap().parse().unwrap();
    assert!(contrast >= 4.5, "worst contrast {contrast}");
    assert!(result(&mut runtime, "hex").unwrap().starts_with('#'));
}

#[test]
fn the_source_colour_is_the_one_the_picture_is_about() {
    let mut runtime = runtime(
        r##"
        -- Mostly black, a teal band, a hint of purple: the teal, whose share
        -- and chroma outweigh the purple's.
        local picked = material.score {
            { color = "#0b0b0b", fraction = 0.8 },
            { color = "#148c96", fraction = 0.15 },
            { color = "#28243c", fraction = 0.05 },
        }
        results.teal = picked[1]:hex()
        -- Nothing with colour: the fallback.
        results.grey = material.score({ { color = "#808080", fraction = 1 } })[1]:hex()
        -- Four vivid hues: four colours, apart.
        results.count = tostring(#material.score {
            { color = "#ff0050", fraction = 0.25 }, { color = "#00c83c", fraction = 0.25 },
            { color = "#1e3cff", fraction = 0.25 }, { color = "#ffdc00", fraction = 0.25 },
        })
    "##,
    );
    assert_eq!(result(&mut runtime, "teal").unwrap(), "#148c96");
    assert_eq!(result(&mut runtime, "grey").unwrap(), "#4285f4");
    assert_eq!(result(&mut runtime, "count").unwrap(), "4");
}

#[test]
fn a_scheme_from_a_picture() {
    let dir = std::env::temp_dir().join(format!("morf-lua-material-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sun.png");
    let mut rgba = Vec::with_capacity(64 * 64 * 4);
    for y in 0..64u32 {
        for x in 0..64u32 {
            let sun = (40..52).contains(&x) && (6..18).contains(&y);
            rgba.extend_from_slice(if sun {
                &[240, 120, 30, 255]
            } else {
                &[240, 240, 240, 255]
            });
        }
    }
    save_rgba(64, 64, rgba, &path, OutputFormat::Png, 90).unwrap();
    let mut runtime = runtime(&format!(
        r#"
        material.from_image("{}", {{ mode = "light" }}, function(ok, s)
            if not ok then results.scheme = "error:" .. tostring(s) return end
            local h = s.primary:hct()
            results.scheme = s.mode .. string.format(" %.0f", h)
        end)
        "#,
        path.display()
    ));
    let deadline = Instant::now() + Duration::from_secs(30);
    let answer = loop {
        runtime.poll_services();
        if let Some(answer) = result(&mut runtime, "scheme") {
            break answer;
        }
        assert!(Instant::now() < deadline, "no scheme");
        std::thread::sleep(Duration::from_millis(5));
    };
    let (mode, hue) = answer.split_once(' ').expect(&answer);
    assert_eq!(mode, "light");
    // The sun's orange: the grey ground has no colour to count.
    let hue: f64 = hue.parse().unwrap();
    assert!((30.0..80.0).contains(&hue), "{answer}");
    std::fs::remove_dir_all(&dir).unwrap();
}
