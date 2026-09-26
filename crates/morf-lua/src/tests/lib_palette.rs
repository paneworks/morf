//! `library/lib/palette.lua`: a desk's colours derived from a picture, in
//! pure Lua over `morf.image`, `morf.color`, `morf.fs` and `morf.json`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use morf_image::ops::{OutputFormat, save_rgba};

use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("morf-lua-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A 64x64 picture painted by `colour(x, y)`.
fn picture(path: &Path, colour: impl Fn(u32, u32) -> [u8; 3]) {
    let mut rgba = Vec::with_capacity(64 * 64 * 4);
    for y in 0..64 {
        for x in 0..64 {
            let [r, g, b] = colour(x, y);
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    save_rgba(64, 64, rgba, path, OutputFormat::Png, 90).unwrap();
}

/// Four pictures that pull a derivation in different directions: a night
/// scene (mostly near-black, one teal band), a pale beach (mostly sand
/// white, an orange sun), a poster (four vivid blocks) and a photograph
/// with no colour in it at all.
fn pictures(dir: &Path) {
    picture(&dir.join("dark.png"), |x, y| {
        if (24..34).contains(&y) {
            [20, 140, 150]
        } else if x < 8 {
            [40, 36, 60]
        } else {
            [8, 10, 14]
        }
    });
    picture(&dir.join("light.png"), |x, y| {
        if (40..52).contains(&x) && (6..18).contains(&y) {
            [240, 120, 30]
        } else if y > 50 {
            [200, 190, 170]
        } else {
            [246, 240, 228]
        }
    });
    picture(&dir.join("saturated.png"), |x, y| match (x < 32, y < 32) {
        (true, true) => [255, 0, 80],
        (false, true) => [0, 200, 60],
        (true, false) => [30, 60, 255],
        (false, false) => [255, 220, 0],
    });
    picture(&dir.join("grey.png"), |x, _| {
        let v = (x * 4) as u8;
        [v, v, v]
    });
}

fn runtime() -> Runtime {
    let mut runtime = Runtime::default();
    runtime.set_module_roots(vec![
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library"),
    ]);
    runtime
}

/// Pumps the loop until the configuration's `results[key]` is set.
fn pump(runtime: &mut Runtime, key: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        runtime.poll_services();
        let value = runtime
            .call_ipc("result", &[IpcValue::String(key.to_owned())])
            .unwrap();
        if let [IpcValue::String(text)] = value.as_slice() {
            return text.clone();
        }
        if Instant::now() >= deadline {
            let logs: Vec<_> = runtime
                .take_logs()
                .into_iter()
                .map(|entry| entry.message)
                .collect();
            panic!("`{key}` never answered: {logs:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

const PRELUDE: &str = r#"
    local palette = require("lib.palette")
    results = {}
    morf.ipc.result = function(key) return results[key] end
    -- Runs `body(p)` in the answer; whatever it returns (or raises) is the
    -- result, so a failure names itself instead of timing out.
    local function answer(key, body)
        return function(ok, p)
            if not ok then results[key] = "error:" .. tostring(p) return end
            local fine, value = pcall(body, p)
            results[key] = fine and tostring(value) or ("error:" .. tostring(value))
        end
    end
    local function shortfalls(p)
        local report, all = palette.check(p)
        if all then return "ok" end
        local out = {}
        for _, row in ipairs(report) do
            if not row.ok then
                out[#out + 1] = string.format("%s on %s %.2f < %g", row.fg, row.bg, row.ratio, row.minimum)
            end
        end
        return table.concat(out, "; ")
    end
"#;

#[test]
fn every_picture_keeps_the_contrast_floors_in_both_modes() {
    let dir = scratch("palette-floors");
    pictures(&dir);
    let mut runtime = runtime();
    let source = format!(
        r##"{PRELUDE}
        for _, name in ipairs({{ "dark", "light", "saturated", "grey" }}) do
            for _, mode in ipairs({{ "dark", "light" }}) do
                assert(palette.from_image("{base}/" .. name .. ".png", {{ mode = mode, cache = false }},
                    answer(name .. "." .. mode, function(p)
                        assert(p.mode == mode, p.mode)
                        for _, token in ipairs(palette.TOKENS) do assert(p[token], token) end
                        for _, slot in ipairs(palette.TERMINAL) do assert(p.terminal[slot], slot) end
                        return shortfalls(p) .. " accent=" .. p.accent:hex() .. " picked=" .. p.picked
                    end)))
            end
        end
        "##,
        base = dir.display()
    );
    runtime.execute("floors.lua", source.as_bytes()).unwrap();
    let mut picks = std::collections::BTreeMap::new();
    for name in ["dark", "light", "saturated", "grey"] {
        for mode in ["dark", "light"] {
            let key = format!("{name}.{mode}");
            let result = pump(&mut runtime, &key);
            assert!(result.starts_with("ok "), "{key}: {result}");
            picks.insert(key, result);
        }
    }
    // The night scene's accent is its teal, not the purple edge or the black.
    assert!(picks["dark.dark"].contains("picked=#148c96"), "{picks:?}");
    // The beach's is its sun.
    assert!(picks["light.dark"].contains("picked=#f0781e"), "{picks:?}");
    // A picture with no colour falls back to the neutral accent.
    assert!(picks["grey.dark"].contains("picked=#89b4fa"), "{picks:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// `rule = "impasto"` is theme_manager.py's `build_palette` token for token:
/// the accent it picks, grounds stepped from it, fixed type.
#[test]
fn the_impasto_rule_is_the_originals_arithmetic() {
    let dir = scratch("palette-impasto");
    pictures(&dir);
    let mut runtime = runtime();
    let source = format!(
        r##"{PRELUDE}
        assert(palette.from_image("{base}/dark.png", {{ rule = "impasto", cache = false }},
            answer("dark", function(p)
                return table.concat({{ p.accent:hex(), p.background:hex(), p.surface:hex(),
                    p.surfaceHover:hex(), p.border:hex(), p.text:hex(), p.textMuted:hex(),
                    p.accentText:hex() }}, " ")
            end)))
        "##,
        base = dir.display()
    );
    runtime.execute("impasto.lua", source.as_bytes()).unwrap();
    let result = pump(&mut runtime, "dark");
    // The teal band, quantised to (20, 140, 149): background (17 + 20·.04,
    // 19 + 140·.04, 24 + 149·.05) floored, then +14, +16/+16/+18, +16/+16/+20;
    // its luma is just over 0.45, so the type on it is the dark one.
    assert_eq!(
        result, "#148c95 #11181f #1f262d #2f363f #3f4653 #eef2f7 #94a1b2 #11111b",
        "{result}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_picture_gives_the_same_palette_every_time_and_the_second_is_cached() {
    let dir = scratch("palette-cache");
    pictures(&dir);
    let mut runtime = runtime();
    let source = format!(
        r##"{PRELUDE}
        local path = "{base}/saturated.png"
        palette.from_image(path, {{ cache = false }}, answer("first", palette.build.json))
        palette.from_image(path, {{ cache = false }}, answer("second", palette.build.json))
        palette.from_image(path, {{ cache_dir = "{base}/cache" }}, answer("fresh", function(p)
            assert(p.cached == false)
            -- The same file, asked again: answered from the cache before the
            -- call returns.
            local now
            palette.from_image(path, {{ cache_dir = "{base}/cache" }}, function(ok, again)
                assert(ok and again.cached == true)
                now = palette.build.json(again)
            end)
            assert(now, "the cached palette answered at once")
            assert(now == palette.build.json(p), "the cache gives back what it was given")
            -- Other options are another palette.
            local other
            palette.from_image(path, {{ cache_dir = "{base}/cache", mode = "light" }}, function(ok, q)
                other = q and q.cached
            end)
            assert(other == nil, "a different mode is not served from the dark one's cache")
            return "ok"
        end))
        local missing_ok, missing_err
        local r = palette.from_image("{base}/nope.png", function(ok, err) missing_ok, missing_err = ok, err end)
        assert(r == nil and missing_ok == false and type(missing_err) == "string")
        "##,
        base = dir.display()
    );
    runtime.execute("cache.lua", source.as_bytes()).unwrap();
    let first = pump(&mut runtime, "first");
    let second = pump(&mut runtime, "second");
    assert!(first.contains("\"accent\""), "{first}");
    assert_eq!(first, second);
    assert_eq!(pump(&mut runtime, "fresh"), "ok");
    let cached: Vec<_> = std::fs::read_dir(dir.join("cache")).unwrap().collect();
    assert_eq!(cached.len(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn writers_produce_files_their_readers_parse() {
    let dir = scratch("palette-writers");
    let mut runtime = runtime();
    let source = format!(
        r##"{PRELUDE}
        local p = palette.from_accent("#3366cc")
        local base = "{base}"
        local plain = morf.json.decode(palette.build.json(p))
        assert(plain.accent == p.accent:hex() and plain.terminal.color4 == p.terminal.color4:hex())
        local wal = morf.json.decode(palette.build.pywal(p))
        for slot = 0, 15 do assert(wal.colors["color" .. slot]:match("^#%x%x%x%x%x%x$"), slot) end
        assert(wal.special.background == p.terminal.background:hex())

        local kitty = palette.build.kitty(p)
        local slots = 0
        for line in kitty:gmatch("[^\n]+") do
            if not line:match("^#") then
                assert(line:match("^[%w_]+ #%x%x%x%x%x%x$"), line)
                if line:match("^color%d+ ") then slots = slots + 1 end
            end
        end
        assert(slots == 16, slots)

        local foot = palette.build.foot(p)
        assert(foot:match("\nregular7=%x%x%x%x%x%x\n") and foot:match("\nbright7=%x+") and foot:match("%[cursor%]"))
        local toml = palette.build.alacritty(p)
        assert(toml:match('%[colors%.normal%]\nblack = "#%x+"') and toml:match('%[colors%.bright%]'))
        local btop = palette.build.btop(p)
        local themes = 0
        for _ in btop:gmatch('theme%[[%w_]+%]="#%x%x%x%x%x%x"') do themes = themes + 1 end
        assert(themes == 48, themes)
        assert(palette.build.cava(p):match("gradient_color_4 = '#%x+'"))
        local gtk = palette.build.gtk(p)
        assert(gtk:match("@define%-color accent_bg_color #%x+;") and gtk:match("@define%-color theme_bg_color #%x+;"))
        local lua = palette.build.lua(p)
        local chunk = assert(load(lua))
        local back = chunk()
        assert(back.accent == p.accent:hex() and back.terminal.color1 == p.terminal.color1:hex())

        -- Writes land atomically where they are told, parents made.
        assert(palette.write.kitty(p, base .. "/kitty/colors.conf"))
        assert(morf.fs.read(base .. "/kitty/colors.conf") == kitty)
        assert(palette.write.pywal(p, base .. "/wal/colors.json"))
        assert(palette.write.template("x={{{{accent.strip}}}}\n", p, base .. "/t/out"))
        assert(morf.fs.read(base .. "/t/out") == "x=" .. p.accent:hex():sub(2) .. "\n")
        assert(palette.reload_hints.cava.signal == "SIGUSR2")
        results.done = "ok"
        "##,
        base = dir.display()
    );
    runtime.execute("writers.lua", source.as_bytes()).unwrap();
    assert_eq!(pump(&mut runtime, "done"), "ok");
    let json = std::fs::read_to_string(dir.join("wal/colors.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["colors"]["color15"].is_string());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn templates_format_and_filter() {
    let mut runtime = runtime();
    let source = format!(
        r##"{PRELUDE}
        local p = palette.from_accent("#3366cc")
        local a = p.accent
        local rgb = a:rgb8()
        local function eq(template, expected)
            local got = palette.render(template, p)
            assert(got == expected, template .. " gave " .. got .. ", not " .. expected)
        end
        eq("{{{{accent}}}}", a:hex())
        eq("{{{{ accent.hex }}}}", a:hex())
        eq("{{{{accent.rgb}}}}", rgb.r .. "," .. rgb.g .. "," .. rgb.b)
        eq("{{{{accent.strip}}}}", a:hex():sub(2))
        eq("{{{{accent.r}}}}", tostring(rgb.r))
        eq("{{{{accent | lighten 0.1}}}}", a:lighten(0.1):hex())
        eq("{{{{accent | darken 0.2 | strip | upper}}}}", a:darken(0.2):hex():sub(2):upper())
        eq("{{{{accent | mix background 0.5}}}}", a:mix(p.background, 0.5, "oklab"):hex())
        eq("{{{{accent | mix #ffffff 0.25 oklch}}}}", a:mix("#ffffff", 0.25, "oklch"):hex())
        eq("{{{{accent | alpha 0.5 | hexa}}}}", a:hex() .. "80")
        eq("{{{{terminal.color1.xhex}}}}", "0x" .. p.terminal.color1:hex():sub(2))
        eq("mode={{{{mode}}}}; {{{{text}}}} on {{{{background}}}}", "mode=dark; " .. p.text:hex() .. " on " .. p.background:hex())
        assert(not pcall(palette.render, "{{{{nope}}}}", p))
        assert(not pcall(palette.render, "{{{{accent | twirl}}}}", p))
        assert(not pcall(palette.render, "{{{{accent.nope}}}}", p))
        results.done = "ok"
        "##
    );
    runtime.execute("templates.lua", source.as_bytes()).unwrap();
    assert_eq!(pump(&mut runtime, "done"), "ok");
}

#[test]
fn presets_accents_and_blends() {
    let mut runtime = runtime();
    let source = format!(
        r##"{PRELUDE}
        assert(#palette.presets == 9)
        for _, entry in ipairs(palette.presets) do
            local p = assert(palette.preset(entry.id))
            assert(p.accent:hex() == entry.colors.accent, entry.id)
            for _, slot in ipairs(palette.TERMINAL) do assert(p.terminal[slot], entry.id .. " " .. slot) end
            -- The terminal set is derived, so it keeps its floors even where
            -- the scheme's own tokens are the author's choice.
            local report = palette.check(p)
            for _, row in ipairs(report) do
                if row.fg:match("^terminal%.color") then
                    assert(row.ok, entry.id .. ": " .. row.fg .. " " .. row.ratio)
                end
            end
        end
        assert(palette.preset("catppuccin_latte").mode == "light")
        assert(palette.preset("nope") == nil)

        -- From one colour: its hue is the accent's, and the floors hold.
        for _, colour in ipairs({{ "#3366cc", "#ffee00", "#ff0033", "#777777", "#00ff88" }}) do
            for _, mode in ipairs({{ "dark", "light" }}) do
                local p = palette.from_accent(colour, {{ mode = mode }})
                local _, all = palette.check(p)
                assert(all, colour .. " " .. mode .. ": " .. shortfalls(p))
            end
        end
        local blue = palette.from_accent("#3366cc")
        local hue = blue.accent:oklch().h
        assert(math.abs(hue - morf.color("#3366cc"):oklch().h) < 3, hue)

        -- Blends: the ends are the ends, the middle is between.
        local a, b = palette.from_accent("#3366cc"), palette.preset("gruvbox_dark")
        local start, stop, middle = palette.blend(a, b, 0), palette.blend(a, b, 1), palette.blend(a, b, 0.5)
        for _, name in ipairs(palette.TOKENS) do
            assert(start[name]:hex() == a[name]:hex(), name)
            assert(stop[name]:hex() == b[name]:hex(), name)
        end
        assert(middle.accent:hex() == a.accent:mix(b.accent, 0.5, "oklab"):hex())
        assert(middle.terminal.color3:hex() ~= a.terminal.color3:hex())
        -- Hex tables blend too, for a palette read back from a file.
        assert(palette.blend(palette.to_hex(a), palette.to_hex(b), 1).accent:hex() == b.accent:hex())
        results.done = "ok"
        "##
    );
    runtime.execute("presets.lua", source.as_bytes()).unwrap();
    assert_eq!(pump(&mut runtime, "done"), "ok");
}

/// A real wallpaper, when there is one. Read only; writes nowhere.
/// `cargo test -p morf-lua real_wallpaper -- --ignored --nocapture`
#[test]
#[ignore]
fn real_wallpaper() {
    let path = std::env::var("PALETTE_WALLPAPER")
        .unwrap_or_else(|_| "/env/set/.wallpaper/cross000.png".to_owned());
    if !Path::new(&path).exists() {
        return;
    }
    let mut runtime = runtime();
    let source = format!(
        r##"{PRELUDE}
        for _, mode in ipairs({{ "dark", "light" }}) do
            palette.from_image("{path}", {{ mode = mode, cache = false }}, answer(mode, function(p)
                local lines = {{ "check: " .. shortfalls(p), "picked " .. p.picked,
                    "swatches " .. table.concat(p.swatches, " ") }}
                for _, name in ipairs(palette.TOKENS) do
                    lines[#lines + 1] = string.format("%-13s %s", name, p[name]:hex())
                end
                for slot = 0, 15 do
                    lines[#lines + 1] = string.format("color%-8d %s", slot, p.terminal["color" .. slot]:hex())
                end
                return table.concat(lines, "\n")
            end))
        end
        "##
    );
    runtime.execute("real.lua", source.as_bytes()).unwrap();
    for mode in ["dark", "light"] {
        let result = pump(&mut runtime, mode);
        eprintln!("--- {mode}\n{result}");
        assert!(result.starts_with("check: ok"), "{result}");
    }
}
