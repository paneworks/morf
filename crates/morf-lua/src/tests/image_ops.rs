//! `morf.image` and `morf.screencopy.save`, answered on the main loop.

use std::time::{Duration, Instant};

use morf_image::ops::{OutputFormat, save_rgba};

use super::*;

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

#[test]
fn a_capture_saved_to_a_file_is_cut_and_encoded_on_a_worker() {
    let dir = scratch("capture-save");
    let mut runtime = Runtime::default();
    let source = format!(
        r##"{PRELUDE}
        morf.screencopy.save {{
            path = "{base}/shot.png", output = "HDMI-A-1",
            region = {{ x = 1, y = 0, w = 2, h = 2 }}, on_done = answer("shot"),
        }}
        morf.screencopy.save {{ path = "{base}/failed.png", on_done = answer("failed") }}
        assert(not pcall(morf.screencopy.save, {{ path = "{base}/x.bmp" }}))
        assert(not pcall(morf.screencopy.save, {{ path = "{base}/x.png", region = {{ 0, 0, 0, 4 }} }}))
        "##,
        base = dir.display()
    );
    runtime.execute("save.lua", source.as_bytes()).unwrap();
    let requests = runtime.take_screencopy_requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].output.as_deref(), Some("HDMI-A-1"));
    assert!(!requests[0].gpu);
    assert!(!runtime.screencopy_publishes(requests[0].id));

    // Three pixels a row, two rows, stored bottom row first, as BGRX with
    // padding at the end of each row: what a compositor may well hand over.
    let row = |b: u8| {
        let mut row = Vec::new();
        for x in 0..3u8 {
            row.extend_from_slice(&[b, x * 10, 200, 0]);
        }
        row.extend_from_slice(&[9, 9, 9, 9]);
        row
    };
    let mut pixels = row(2);
    pixels.extend(row(1));
    assert!(runtime.dispatch_screencopy(
        requests[0].id,
        Ok(Screencopy {
            width: 3,
            height: 2,
            stride: 16,
            format: "xrgb8888".to_owned(),
            y_invert: true,
            gpu: false,
            source: String::new(),
            pixels,
        })
    ));
    assert!(runtime.dispatch_screencopy(requests[1].id, Err("no output".to_owned())));
    assert_eq!(pump(&mut runtime, "failed"), "error:no output");
    assert_eq!(pump(&mut runtime, "shot"), "2x2:png");

    // Top row came from the last stored row; red and blue swapped back; the
    // padding byte is not taken for alpha.
    let pixel = morf_image::ops::pixel_at(dir.join("shot.png"), 0, 0, u64::MAX).unwrap();
    assert_eq!(pixel, [200, 10, 1, 255]);
    let pixel = morf_image::ops::pixel_at(dir.join("shot.png"), 1, 1, u64::MAX).unwrap();
    assert_eq!(pixel, [200, 20, 2, 255]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pixels_become_a_source_every_renderer_can_draw() {
    let dir = scratch("raw");
    let mut runtime = Runtime::default();
    let source = format!(
        r##"
        local img = morf.image
        -- Two pixels, as a string: red, then half-transparent green.
        local src = assert(img.from_rgba("\255\0\0\255\0\255\0\128", 2, 1))
        assert(src:find("^memory:image/"))
        -- A notification's image-data, as morf.dbus hands it over: an
        -- (iiibiiay) struct with the bytes as a list, rows padded to 8.
        local dbus = {{ 2, 2, 8, false, 8, 3, {{ 1,2,3, 4,5,6, 0,0, 7,8,9, 10,11,12 }} }}
        local note = assert(img.from_dbus(dbus, {{ name = "note-1" }}))
        local named = assert(img.from_dbus({{ width = 1, height = 1, rowstride = 4,
          has_alpha = true, bits_per_sample = 8, channels = 4, data = "\9\9\9\255" }}))
        local again = assert(img.from_dbus(dbus, {{ name = "note-1" }}))
        assert(again ~= note, "a republished name is a new source")
        _G.sources = {{ src = src, note = note, again = again, named = named }}
        local short, why = img.from_rgba("\0\0\0", 1, 1)
        assert(short == nil and why:find("need 4 bytes"), why)
        assert(select(2, img.from_dbus({{ 1, 1, 2, false, 16, 1, "\0\0" }})):find("16 bits"))
        assert(not pcall(img.from_rgba, "\0\0\0\0", 1, 1, nil, {{ format = "yuv" }}))
        assert(img.encode_png("\1\2\3\255\4\5\6\255", 2, 1, "{dir}/two.png"))
        assert(img.encode_png("\3\2\1\255", 1, 1, "{dir}/bgra.png", {{ format = "bgra" }}))
        assert(img.release(named) and not img.release(named))
        assert(not img.release("memory:image/not-ours"))
        "##,
        dir = dir.display()
    );
    runtime.execute("raw.lua", source.as_bytes()).unwrap();
    let get = |runtime: &mut Runtime, key: &str| -> String {
        runtime
            .execute(
                "get.lua",
                format!("morf.ipc.got = function() return _G.sources.{key} end").as_bytes(),
            )
            .unwrap();
        match runtime.call_ipc("got", &[]).unwrap().as_slice() {
            [IpcValue::String(value)] => value.clone(),
            other => panic!("{other:?}"),
        }
    };
    let (src, note, again, named) = (
        get(&mut runtime, "src"),
        get(&mut runtime, "note"),
        get(&mut runtime, "again"),
        get(&mut runtime, "named"),
    );
    let mut cache = morf_image::ImageCache::default();
    assert_eq!(cache.intrinsic_size(&src).unwrap(), (2, 1));
    assert_eq!(
        cache.load(&src, 2, 1, 120).unwrap().rgba,
        [255, 0, 0, 255, 0, 255, 0, 128]
    );
    assert!(cache.load(&note, 2, 2, 120).is_err(), "replaced by `again`");
    assert_eq!(
        cache.load(&again, 2, 2, 120).unwrap().rgba,
        [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
    );
    assert!(cache.load(&named, 1, 1, 120).is_err(), "released");
    assert_eq!(
        morf_image::ops::pixel_at(dir.join("two.png"), 1, 0, u64::MAX).unwrap(),
        [4, 5, 6, 255]
    );
    assert_eq!(
        morf_image::ops::pixel_at(dir.join("bgra.png"), 0, 0, u64::MAX).unwrap(),
        [1, 2, 3, 255]
    );
    drop(runtime);
    assert!(
        cache.load(&src, 2, 1, 120).is_err() && cache.load(&again, 2, 2, 120).is_err(),
        "a runtime that goes takes its pictures with it"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn files_read_async_arrive_in_order_on_a_later_turn() {
    let dir = scratch("read-async");
    std::fs::write(dir.join("one"), "first\n").unwrap();
    std::fs::write(dir.join("two"), "second").unwrap();
    let mut runtime = Runtime::default();
    let source = format!(
        r##"
        results = {{}}
        morf.ipc.result = function(key) return results[key] end
        local base = "{base}"
        local sync = false
        assert(morf.fs.read_async({{ base .. "/one", base .. "/missing", base .. "/two" }},
            function(ok, contents)
                assert(ok and #contents == 3)
                results.files = contents[1] .. "|" .. tostring(contents[2]) .. "|" .. contents[3]
                    .. "|" .. tostring(sync)
            end))
        assert(morf.fs.read_async(base .. "/two", function(ok, contents)
            results.one = tostring(contents[1])
        end, 3))
        -- Never within the call itself.
        sync = true
        assert(not pcall(morf.fs.read_async, {{ 7 }}, function() end))
        assert(not pcall(morf.fs.read_async, base .. "/one", function() end, -1))
        "##,
        base = dir.display()
    );
    runtime.execute("read.lua", source.as_bytes()).unwrap();
    assert_eq!(pump(&mut runtime, "files"), "first\n|false|second|true");
    assert_eq!(
        pump(&mut runtime, "one"),
        "false",
        "over the limit is not read"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
