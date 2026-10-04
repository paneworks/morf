//! Tests of image_ops.rs that reach the runtime's internals; the rest are in
//! tests/engine/image_ops.rs.
#![allow(unused_imports)]

use super::*;
use morf_image::ops::{OutputFormat, save_rgba};
use std::time::{Duration, Instant};

#[test]
fn image_work_runs_off_the_loop_and_answers_on_it() {
    let dir = scratch("image-ops");
    two_tone(&dir.join("a.png"));
    let mut runtime = Runtime::default();
    let source = format!(
        r##"{PRELUDE}
        local base = "{base}"
        local info = morf.image.info(base .. "/a.png")
        assert(info.width == 40 and info.height == 20 and info.format == "png")
        local missing, err = morf.image.info(base .. "/nope.png")
        assert(missing == nil and type(err) == "string")

        assert(morf.image.process {{
            source = base .. "/a.png",
            ops = {{ {{"square"}}, {{"resize", 10, 10, "exact"}}, {{"rotate", 90}},
                    {{"flip", "h"}}, {{"blur", 0.5}}, {{"grayscale"}} }},
            output = base .. "/thumb.jpg",
            quality = 70,
            on_done = answer("process"),
        }})
        morf.image.process {{
            source = base .. "/a.png", ops = {{ {{"crop", 100, 100, 5, 5}} }},
            output = base .. "/never.png", on_done = answer("miss"),
        }}
        assert(morf.image.palette(base .. "/a.png", 2, answer("palette")))
        assert(morf.image.pixel(base .. "/a.png", 39, 0, answer("pixel")))
        assert(morf.image.pixel(base .. "/a.png", 0, 0):hex() == "#ff0000")
        local outside, why = morf.image.pixel(base .. "/a.png", 99, 0)
        assert(outside == nil and why:find("outside"))

        -- Mistakes in the call raise; nothing is queued for them.
        assert(not pcall(morf.image.process, {{ source = base .. "/a.png", output = base .. "/x.gif" }}))
        assert(not pcall(morf.image.process, {{ source = base .. "/a.png", output = base .. "/x.png",
            ops = {{ {{"twirl"}} }} }}))
        assert(not pcall(morf.image.process, {{ source = base .. "/a.png", output = base .. "/x.png",
            ops = {{ {{"rotate", 45}} }} }}))
        assert(not pcall(morf.image.palette, base .. "/a.png", 0, answer("never")))
        assert(not pcall(morf.image.palette, base .. "/a.png", 3))
        assert(morf.image.limits.max_dimension == 16384)
        "##,
        base = dir.display()
    );
    runtime.execute("image.lua", source.as_bytes()).unwrap();

    assert_eq!(pump(&mut runtime, "process"), "10x10:jpeg");
    assert!(dir.join("thumb.jpg").exists());
    assert!(pump(&mut runtime, "miss").starts_with("error:"));
    assert!(!dir.join("never.png").exists());
    assert_eq!(pump(&mut runtime, "pixel"), "#0000ff");
    let palette = pump(&mut runtime, "palette");
    assert!(
        palette == "#ff0000@0.50,#0000ff@0.50" || palette == "#0000ff@0.50,#ff0000@0.50",
        "{palette}"
    );
    assert_eq!(runtime.reactive.borrow().image_jobs.in_flight(), 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_full_queue_is_refused_and_a_dropped_runtime_is_left_alone() {
    let dir = scratch("image-queue");
    two_tone(&dir.join("a.png"));
    let mut runtime = Runtime::default();
    let source = format!(
        r##"{PRELUDE}
        local refused
        for i = 1, 40 do
            local ok, err = morf.image.process {{
                source = "{base}/a.png", ops = {{ {{"resize", 400, 0}}, {{"blur", 8}} }},
                output = "{base}/out" .. i .. ".png",
            }}
            if not ok then refused = err break end
        end
        assert(refused and refused:find("full"), refused)
        morf.image.palette("{base}/a.png", 4, function() error("never called") end)
        "##,
        base = dir.display()
    );
    runtime.execute("queue.lua", source.as_bytes()).unwrap();
    assert_eq!(
        runtime.reactive.borrow().image_jobs.in_flight(),
        crate::image_jobs::MAX_IN_FLIGHT
    );
    // Gone with work still queued: the workers finish or stop on their own,
    // and no callback runs against a Lua state that no longer exists.
    drop(runtime);
    std::thread::sleep(Duration::from_millis(50));
    let _ = std::fs::remove_dir_all(&dir);
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("morf-lua-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A 40x20 PNG: left half red, right half blue.
fn two_tone(path: &std::path::Path) {
    let mut rgba = Vec::with_capacity(40 * 20 * 4);
    for _ in 0..20 {
        for x in 0..40 {
            rgba.extend_from_slice(if x < 20 {
                &[255, 0, 0, 255]
            } else {
                &[0, 0, 255, 255]
            });
        }
    }
    save_rgba(40, 20, rgba, path, OutputFormat::Png, 90).unwrap();
}

/// Pumps the loop until the configuration's `results[key]` is set.
fn pump(runtime: &mut Runtime, key: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        runtime.poll_services();
        let value = runtime
            .call_ipc("result", &[IpcValue::String(key.to_owned())])
            .unwrap();
        if let [IpcValue::String(text)] = value.as_slice() {
            return text.clone();
        }
        assert!(Instant::now() < deadline, "`{key}` never answered");
        std::thread::sleep(Duration::from_millis(5));
    }
}

const PRELUDE: &str = r#"
    results = {}
    morf.ipc.result = function(key) return results[key] end
    local function answer(key)
        return function(ok, value)
            if not ok then results[key] = "error:" .. value return end
            if type(value) == "table" and value.width then
                results[key] = value.width .. "x" .. value.height .. ":" .. value.format
            elseif type(value) == "table" then
                local parts = {}
                for _, entry in ipairs(value) do
                    parts[#parts + 1] = entry.color:hex() .. "@" .. string.format("%.2f", entry.fraction)
                end
                results[key] = table.concat(parts, ",")
            else
                results[key] = value:hex()
            end
        end
    end
"#;
