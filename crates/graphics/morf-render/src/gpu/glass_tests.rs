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
