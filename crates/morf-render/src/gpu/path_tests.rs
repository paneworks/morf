//! A `Path` drawn on a real adapter: filled, stroked past its box, trimmed,
//! and sharp when a transform scales it up.

use morf_layout::{Layout, Size};
use morf_scene::{Element, NodeHandle, Scene, Value};

use super::field_tests::read_frame;
use super::*;
use crate::tests::NoText;
use crate::*;

fn frame(scene: &Scene, root: NodeHandle) -> Vec<u8> {
    let layout = Layout::compute(
        scene,
        root,
        Size {
            width: 64.0,
            height: 64.0,
        },
        &mut NoText,
    )
    .unwrap();
    let list = DrawList::from_scene(scene, &layout).unwrap();
    let mut backend = pollster::block_on(WgpuBackend::new(64, 64)).unwrap();
    read_frame(&mut backend, &list, 64)
}

fn at(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
    let offset = (y * 64 + x) * 4;
    pixels[offset..offset + 4].try_into().unwrap()
}

fn scene_with(path: impl FnOnce(&mut Scene, NodeHandle)) -> (Scene, NodeHandle) {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", 64.0).unwrap();
    scene.assign(root, "height", 64.0).unwrap();
    let node = scene.create(Element::Path);
    path(&mut scene, node);
    scene.reparent(node, Some(root)).unwrap();
    (scene, root)
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_path_fills_its_outline_through_a_view_box() {
    let (scene, root) = scene_with(|scene, node| {
        scene.assign(node, "width", 64.0).unwrap();
        scene.assign(node, "height", 64.0).unwrap();
        // A triangle in a 4-unit box: the left half of the node, top to bottom.
        scene.assign(node, "d", "M0 0 L2 2 L0 4 Z").unwrap();
        scene
            .assign(
                node,
                "view_box",
                Value::List(vec![0.0.into(), 0.0.into(), 4.0.into(), 4.0.into()]),
            )
            .unwrap();
        scene.assign(node, "fill_color", "#00ff00").unwrap();
    });
    let pixels = frame(&scene, root);
    assert_eq!(at(&pixels, 8, 32), [0, 255, 0, 255], "inside the triangle");
    assert_eq!(at(&pixels, 48, 32)[3], 0, "right of its tip");
    assert_eq!(at(&pixels, 30, 4)[3], 0, "above its upper edge");
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_stroke_reaches_past_the_box_and_a_trim_cuts_it() {
    let (scene, root) = scene_with(|scene, node| {
        scene.assign(node, "x", 16.0).unwrap();
        scene.assign(node, "y", 16.0).unwrap();
        scene.assign(node, "width", 32.0).unwrap();
        scene.assign(node, "height", 32.0).unwrap();
        // Along the node's top edge, 8 wide: half of it lies above the box.
        scene.assign(node, "d", "M0 0 L32 0").unwrap();
        scene.assign(node, "fill_color", "transparent").unwrap();
        scene.assign(node, "stroke_color", "#ffffff").unwrap();
        scene.assign(node, "stroke_width", 8.0).unwrap();
        scene.assign(node, "trim_end", 0.5).unwrap();
    });
    let pixels = frame(&scene, root);
    assert_eq!(
        at(&pixels, 20, 13)[3],
        255,
        "above the box, inside the stroke"
    );
    assert_eq!(at(&pixels, 20, 18)[3], 255, "below the edge");
    assert_eq!(at(&pixels, 40, 16)[3], 0, "past the trim");
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_scaled_path_is_drawn_at_the_size_it_is_seen() {
    // A disc 8 across, scaled four times. Stretched from an 8-pixel picture,
    // its edge would be a ramp several pixels wide; drawn at the size it
    // covers, one pixel of antialiasing is all there is.
    let (scene, root) = scene_with(|scene, node| {
        scene.assign(node, "x", 28.0).unwrap();
        scene.assign(node, "y", 28.0).unwrap();
        scene.assign(node, "width", 8.0).unwrap();
        scene.assign(node, "height", 8.0).unwrap();
        scene.assign(node, "scale", 4.0).unwrap();
        scene.assign(node, "d", "M4 0 A4 4 0 1 1 3.99 0 Z").unwrap();
        scene.assign(node, "fill_color", "#ffffff").unwrap();
    });
    let pixels = frame(&scene, root);
    // Along the middle row, from the centre out past the edge at x = 48.
    let partial = (32..64)
        .filter(|x| {
            let alpha = at(&pixels, *x, 32)[3];
            alpha > 10 && alpha < 245
        })
        .count();
    assert!(partial <= 2, "a crisp edge, not a stretched one: {partial}");
    assert_eq!(at(&pixels, 40, 32)[3], 255);
    assert_eq!(at(&pixels, 52, 32)[3], 0);
}
