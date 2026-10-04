//! Tests of images.rs that reach the runtime's internals; the rest are in
//! tests/engine/images.rs.
#![allow(unused_imports)]

use super::*;
use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, RgbaImage};
use std::time::{Duration, Instant};

#[test]
fn a_gif_plays_while_it_is_drawn_and_stops_when_asked() {
    let dir = scratch("gif");
    let path = dir.join("spin.gif");
    gif(
        &path,
        &[[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]],
        50,
    );
    let mut runtime = Runtime::default();
    let source = format!(
        r##"
        local ui = require("morf.ui")
        _G.spin = ui.Image {{ source = "{path}", width = 16, height = 16 }}
        ui.Item {{ _G.spin }}
        morf.ipc.state = function()
          return ("%s %d/%d"):format(spin.status, spin.frame, spin.frame_count)
        end
        "##,
        path = path.display(),
    );
    runtime.execute("gif.lua", source.as_bytes()).unwrap();
    let mut cache = morf_image::ImageCache::default();
    paint(&mut runtime, &mut cache);
    assert_eq!(ipc(&mut runtime, "state"), "ready 0/3");
    let advance = |runtime: &mut Runtime, at: Instant| {
        crate::images::advance(&mut runtime.reactive.borrow_mut(), at)
    };
    let set =
        |runtime: &mut Runtime, line: &str| runtime.execute("set.lua", line.as_bytes()).unwrap();
    let start = Instant::now();
    advance(&mut runtime, start);
    assert!(!advance(&mut runtime, start + Duration::from_millis(30)));
    assert!(advance(&mut runtime, start + Duration::from_millis(60)));
    assert_eq!(ipc(&mut runtime, "state"), "ready 1/3");
    // 120 ms more: two frames on, back round to the first.
    advance(&mut runtime, start + Duration::from_millis(180));
    assert_eq!(ipc(&mut runtime, "state"), "ready 0/3");
    // Paused, it stays; at double speed it moves in half the time.
    set(&mut runtime, "spin.playing = false");
    assert!(!advance(&mut runtime, start + Duration::from_millis(400)));
    set(&mut runtime, "spin.playing = true spin.speed = 2");
    assert!(advance(&mut runtime, start + Duration::from_millis(430)));
    assert_eq!(ipc(&mut runtime, "state"), "ready 1/3");
    // A write to `frame` seeks.
    set(&mut runtime, "spin.speed = 1 spin.frame = 2");
    advance(&mut runtime, start + Duration::from_millis(440));
    assert_eq!(ipc(&mut runtime, "state"), "ready 2/3");
    // Hidden, it stops where it is.
    set(&mut runtime, "spin.visible = false");
    paint(&mut runtime, &mut cache);
    assert!(!advance(&mut runtime, start + Duration::from_millis(900)));
    assert_eq!(ipc(&mut runtime, "state"), "ready 2/3");
    // Once through, it ends on its last frame.
    set(
        &mut runtime,
        "spin.visible = true spin.loops = 1 spin.frame = 0",
    );
    paint(&mut runtime, &mut cache);
    let again = Instant::now();
    advance(&mut runtime, again);
    advance(&mut runtime, again + Duration::from_millis(1000));
    assert_eq!(ipc(&mut runtime, "state"), "ready 2/3");
    assert!(!advance(&mut runtime, again + Duration::from_millis(2000)));
    // Only the moving picture was kept, once.
    assert_eq!(cache.animation_usage().0, 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("morf-images-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn gif(path: &std::path::Path, colours: &[[u8; 4]], delay_ms: u32) {
    let mut out = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut out);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for colour in colours {
            encoder
                .encode_frame(Frame::from_parts(
                    RgbaImage::from_pixel(4, 4, image::Rgba(*colour)),
                    0,
                    0,
                    Delay::from_numer_denom_ms(delay_ms, 1),
                ))
                .unwrap();
        }
    }
    std::fs::write(path, out).unwrap();
}

/// Lays out the first root and reads the images back, as a paint would.
fn paint(runtime: &mut Runtime, cache: &mut morf_image::ImageCache) {
    let root = runtime.scene().roots()[0];
    let mut text = morf_text::TextSystem::new();
    let layout = runtime
        .compute_layout(
            root,
            morf_layout::Size {
                width: 400.0,
                height: 400.0,
            },
            &mut text,
        )
        .unwrap();
    runtime.sync_images(&layout, cache);
}

fn ipc(runtime: &mut Runtime, verb: &str) -> String {
    match runtime.call_ipc(verb, &[]).unwrap().as_slice() {
        [IpcValue::String(value)] => value.clone(),
        other => panic!("{verb}: {other:?}"),
    }
}
