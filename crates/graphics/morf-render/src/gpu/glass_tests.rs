//! The glass material on a real adapter: its tint lands on the colour it
//! names, refraction moves what is behind, frost takes the detail out, and
//! the rim lights the edge.
//!
//! Run under a Vulkan driver: `nixVulkan cargo test -p morf-render --lib
//! glass -- --include-ignored`.

use morf_layout::{Layout, Size};
use morf_scene::{Element, NodeHandle, Scene, Value};

use super::field_tests::read_frame;
use crate::tests::NoText;
use crate::*;

const SIZE: u32 = 64;

/// A 64-square surface: eight-pixel stripes of `colors` across it, and a
/// transparent 48-square glass at (8, 8) with `glass` assigned to it.
fn desk(colors: [&str; 2], glass: &[(&str, Value)]) -> (Scene, NodeHandle) {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", f64::from(SIZE)).unwrap();
    scene.assign(root, "height", f64::from(SIZE)).unwrap();
    for index in 0..SIZE / 8 {
        let stripe = scene.create(Element::Rect);
        scene.assign(stripe, "x", f64::from(index * 8)).unwrap();
        scene.assign(stripe, "width", 8.0).unwrap();
        scene.assign(stripe, "height", f64::from(SIZE)).unwrap();
        scene
            .assign(stripe, "color", colors[index as usize % 2])
            .unwrap();
        scene.reparent(stripe, Some(root)).unwrap();
    }
    let pane = scene.create(Element::Rect);
    for (property, value) in [("x", 8.0), ("y", 8.0), ("width", 48.0), ("height", 48.0)] {
        scene.assign(pane, property, value).unwrap();
    }
    scene.assign(pane, "color", "#00000000").unwrap();
    for (property, value) in glass {
        scene.assign(pane, property, value.clone()).unwrap();
    }
    scene.reparent(pane, Some(root)).unwrap();
    (scene, root)
}

fn frame(scene: &Scene, root: NodeHandle, blend: BlendSpace) -> Vec<u8> {
    let layout = Layout::compute(
        scene,
        root,
        Size {
            width: f64::from(SIZE),
            height: f64::from(SIZE),
        },
        &mut NoText,
    )
    .unwrap();
    let list = DrawList::from_scene(scene, &layout).unwrap();
    let mut backend = pollster::block_on(WgpuBackend::new(SIZE, SIZE)).unwrap();
    backend.set_blend(blend);
    read_frame(&mut backend, &list, SIZE)
}

fn at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * SIZE + x) * 4) as usize;
    pixels[offset..offset + 4].try_into().unwrap()
}

fn close(pixel: [u8; 4], wanted: [u8; 3], slack: u8) -> bool {
    (0..3).all(|channel| pixel[channel].abs_diff(wanted[channel]) <= slack)
}

/// The luma variance of the 32-square in the middle of the glass, 0 to 1.
fn variance(pixels: &[u8]) -> f64 {
    let lumas: Vec<f64> = (16..48)
        .flat_map(|y| (16..48).map(move |x| (x, y)))
        .map(|(x, y)| f64::from(at(pixels, x, y)[0]) / 255.0)
        .collect();
    let mean = lumas.iter().sum::<f64>() / lumas.len() as f64;
    lumas.iter().map(|luma| (luma - mean).powi(2)).sum::<f64>() / lumas.len() as f64
}

const STRIPES: [&str; 2] = ["#000000ff", "#ffffffff"];

#[test]
#[ignore = "requires a GPU adapter"]
fn clear_glass_bends_the_stripes_at_its_rim_and_leaves_its_middle_sharp() {
    require_gpu!();
    let clear = |refraction: f64| {
        let (scene, root) = desk(
            STRIPES,
            &[
                ("glass_refraction", Value::Number(refraction)),
                ("glass_frost", Value::Number(0.0)),
            ],
        );
        frame(&scene, root, BlendSpace::Linear)
    };
    let flat = clear(0.0);
    let bent = clear(10.0);
    // Just inside the left edge the white stripe under it is drawn from the
    // black one beyond it.
    assert!(at(&flat, 9, 32)[0] > 250, "{:?}", at(&flat, 9, 32));
    assert!(at(&bent, 9, 32)[0] < 5, "{:?}", at(&bent, 9, 32));
    // The middle is past the rim: the stripes are where they were, and sharp.
    for x in 24..40 {
        assert_eq!(at(&bent, x, 32), at(&flat, x, 32), "x = {x}");
        assert!(at(&bent, x, 32)[0] < 3 || at(&bent, x, 32)[0] > 252);
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn frost_takes_the_detail_out_by_as_much_as_it_is_asked() {
    require_gpu!();
    let frosted = |frost: f64| {
        let (scene, root) = desk(
            STRIPES,
            &[
                ("backdrop_blur", Value::Number(6.0)),
                ("glass_frost", Value::Number(frost)),
            ],
        );
        variance(&frame(&scene, root, BlendSpace::Linear))
    };
    let (clear, half, full) = (frosted(0.0), frosted(0.5), frosted(1.0));
    assert!(clear > 0.2, "clear glass keeps the stripes: {clear}");
    assert!(full < clear * 0.05, "frosted: {full} against {clear}");
    assert!(full < half && half < clear, "{clear} > {half} > {full}");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn automatic_frost_blurs_a_busy_backdrop_and_leaves_a_quiet_one_clear() {
    require_gpu!();
    let auto = [
        ("backdrop_blur", Value::Number(6.0)),
        ("glass_frost", Value::from("auto")),
    ];
    let (scene, root) = desk(STRIPES, &auto);
    let busy = variance(&frame(&scene, root, BlendSpace::Linear));
    assert!(busy < 0.01, "black and white stripes are frosted: {busy}");
    // Two greys a few steps apart: too little to frost, so they stay apart.
    let (scene, root) = desk(["#808080ff", "#888888ff"], &auto);
    let quiet = frame(&scene, root, BlendSpace::Linear);
    let (dark, light) = (at(&quiet, 36, 32)[0], at(&quiet, 44, 32)[0]);
    assert!(
        light >= dark + 6,
        "the quiet stripes stay sharp: {dark} and {light}"
    );
    let (scene, root) = desk(
        ["#808080ff", "#888888ff"],
        &[("backdrop_blur", Value::Number(6.0))],
    );
    let frosted = frame(&scene, root, BlendSpace::Linear);
    assert!(at(&frosted, 44, 32)[0].abs_diff(at(&frosted, 36, 32)[0]) <= 3);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_rim_catches_the_light_on_the_side_that_faces_it() {
    require_gpu!();
    let lit = |rim: f64| {
        let (scene, root) = desk(
            ["#202020ff", "#202020ff"],
            &[
                ("glass_rim", Value::Number(rim)),
                ("glass_frost", Value::Number(0.0)),
            ],
        );
        frame(&scene, root, BlendSpace::Linear)
    };
    let (dark, bright) = (lit(0.0), lit(1.0));
    let (top, bottom) = (at(&bright, 32, 8)[0], at(&bright, 32, 55)[0]);
    assert!(top > at(&dark, 32, 8)[0] + 60, "the top edge is lit: {top}");
    assert!(
        top > bottom + 20,
        "more on the top than the bottom: {top}, {bottom}"
    );
    assert!(
        at(&bright, 32, 32)[0] <= at(&dark, 32, 32)[0] + 2,
        "not the middle"
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn an_opaque_tint_is_the_colour_it_names_in_either_blend_space() {
    require_gpu!();
    let glass = [
        ("backdrop_blur", Value::Number(4.0)),
        ("backdrop_tint", Value::from("#3366ccff")),
    ];
    for blend in [BlendSpace::Linear, BlendSpace::Srgb] {
        let (scene, root) = desk(["#808080ff", "#808080ff"], &glass);
        let pixel = at(&frame(&scene, root, blend), 32, 32);
        assert!(
            close(pixel, [0x33, 0x66, 0xcc], 2),
            "{blend:?}: an opaque #3366cc tint drew {pixel:?}"
        );
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_translucent_tint_mixes_as_a_fill_of_that_colour_does() {
    require_gpu!();
    let (scene, root) = desk(
        ["#202020ff", "#202020ff"],
        &[
            ("backdrop_blur", Value::Number(4.0)),
            ("backdrop_tint", Value::from("#3366cc80")),
        ],
    );
    let tinted = at(&frame(&scene, root, BlendSpace::Linear), 32, 32);
    let (mut scene, root) = desk(["#202020ff", "#202020ff"], &[]);
    let fill = scene.create(Element::Rect);
    for (property, value) in [("x", 8.0), ("y", 8.0), ("width", 48.0), ("height", 48.0)] {
        scene.assign(fill, property, value).unwrap();
    }
    scene.assign(fill, "color", "#3366cc80").unwrap();
    scene.reparent(fill, Some(root)).unwrap();
    let filled = at(&frame(&scene, root, BlendSpace::Linear), 32, 32);
    assert!(
        close(tinted, [filled[0], filled[1], filled[2]], 2),
        "a half-strength tint drew {tinted:?}, the same colour as a fill {filled:?}"
    );
}
