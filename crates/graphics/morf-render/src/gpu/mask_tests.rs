//! Alpha masks drawn on a real adapter: a node's subtree composited through
//! another subtree's alpha, or through a gradient, and a frame drawn through
//! the damage an animated mask leaves matching the frame drawn whole.

use morf_layout::{Layout, Size};
use morf_scene::{Color, Element, NodeHandle, Scene, Value};

use crate::tests::NoText;
use crate::*;

const SIZE: u32 = 64;

fn node(scene: &mut Scene, parent: NodeHandle, element: Element, place: [f64; 4]) -> NodeHandle {
    let node = scene.create(element);
    for (property, value) in ["x", "y", "width", "height"].into_iter().zip(place) {
        scene.assign(node, property, value).unwrap();
    }
    scene.reparent(node, Some(parent)).unwrap();
    node
}

/// A green square filling the surface, and its root.
fn green_square() -> (Scene, NodeHandle, NodeHandle) {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", f64::from(SIZE)).unwrap();
    scene.assign(root, "height", f64::from(SIZE)).unwrap();
    let square = node(
        &mut scene,
        root,
        Element::Rect,
        [0.0, 0.0, f64::from(SIZE), f64::from(SIZE)],
    );
    scene.assign(square, "color", "#00ff00ff").unwrap();
    (scene, root, square)
}

/// A circle the size of the surface, given to `owner` as its mask.
fn circle_mask(scene: &mut Scene, owner: NodeHandle) -> NodeHandle {
    let circle = scene.create(Element::Rect);
    scene
        .assign(circle, "radius", f64::from(SIZE) / 2.0)
        .unwrap();
    scene.assign(circle, "color", "#ffffffff").unwrap();
    scene.set_mask(owner, Some(circle)).unwrap();
    circle
}

fn draw(scene: &Scene, root: NodeHandle) -> Vec<u8> {
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
    super::field_tests::render_readback(&list, SIZE)
}

fn at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * SIZE + x) * 4) as usize;
    pixels[offset..offset + 4].try_into().unwrap()
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_node_mask_keeps_what_it_covers() {
    let (mut scene, root, square) = green_square();
    circle_mask(&mut scene, square);
    let pixels = draw(&scene, root);
    assert_eq!(at(&pixels, 32, 32), [0, 255, 0, 255], "the centre shows");
    assert_eq!(at(&pixels, 2, 2)[3], 0, "the corner is cut away");
    assert_eq!(at(&pixels, 61, 61)[3], 0, "every corner");
    // The circle's edge is antialiased into the square's alpha.
    let edge = at(&pixels, 9, 9)[3];
    assert!(
        edge > 0 && edge < 255,
        "the rim at (9, 9) is partly covered: {edge}"
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn an_inverted_mask_cuts_out_what_it_covers() {
    let (mut scene, root, square) = green_square();
    circle_mask(&mut scene, square);
    scene.assign(square, "mask_invert", true).unwrap();
    let pixels = draw(&scene, root);
    assert_eq!(at(&pixels, 32, 32)[3], 0, "the centre is cut out");
    assert_eq!(at(&pixels, 2, 2), [0, 255, 0, 255], "the corner shows");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_gradient_mask_fades_the_edges() {
    let (mut scene, root, square) = green_square();
    let mask = Value::Map(
        [(
            "gradient".to_owned(),
            Value::Map(
                [(
                    "stops".to_owned(),
                    Value::List(vec![
                        Value::List(vec![Value::Number(0.0), Value::Number(0.0)]),
                        Value::List(vec![Value::Number(1.0), Value::Number(0.25)]),
                        Value::List(vec![Value::Number(1.0), Value::Number(0.75)]),
                        Value::List(vec![Value::Number(0.0), Value::Number(1.0)]),
                    ]),
                )]
                .into_iter()
                .collect(),
            ),
        )]
        .into_iter()
        .collect(),
    );
    scene.assign(square, "mask", mask).unwrap();
    let pixels = draw(&scene, root);
    // The default linear gradient runs top to bottom.
    assert!(at(&pixels, 32, 0)[3] < 20, "top {:?}", at(&pixels, 32, 0));
    assert!(
        at(&pixels, 32, 63)[3] < 20,
        "bottom {:?}",
        at(&pixels, 32, 63)
    );
    assert_eq!(at(&pixels, 32, 32)[3], 255, "the middle is whole");
    let fading = at(&pixels, 32, 8)[3];
    assert!(
        fading > 60 && fading < 200,
        "halfway into the fade: {fading}"
    );
    // Rows fade alike across the width.
    assert_eq!(at(&pixels, 4, 8)[3], fading);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_mask_composes_with_opacity_and_a_rounded_clip() {
    let (mut scene, root, square) = green_square();
    circle_mask(&mut scene, square);
    scene.assign(square, "opacity", 0.5).unwrap();
    let pixels = draw(&scene, root);
    let centre = at(&pixels, 32, 32);
    assert!(
        (120..=136).contains(&centre[3]),
        "half the opacity: {centre:?}"
    );
    assert_eq!(at(&pixels, 2, 2)[3], 0);

    // A rounded ClipRect with a mask: the clip and the mask both cut.
    let (mut scene, root, _) = green_square();
    let clip = node(
        &mut scene,
        root,
        Element::ClipRect,
        [0.0, 0.0, f64::from(SIZE), f64::from(SIZE)],
    );
    scene.assign(clip, "radius", 8.0).unwrap();
    scene.assign(clip, "color", "#ff0000ff").unwrap();
    let bar = scene.create(Element::Rect);
    scene.assign(bar, "width", 64.0).unwrap();
    scene.assign(bar, "height", 32.0).unwrap();
    scene.set_mask(clip, Some(bar)).unwrap();
    let pixels = draw(&scene, root);
    assert_eq!(at(&pixels, 32, 16), [255, 0, 0, 255], "inside both");
    // Below the mask's bar the green square beneath shows through.
    assert_eq!(at(&pixels, 32, 48), [0, 255, 0, 255], "outside the mask");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_mask_follows_its_owner_through_a_transform() {
    let (mut scene, root, square) = green_square();
    scene.assign(square, "color", "#00000000").unwrap();
    let moved = node(&mut scene, root, Element::Item, [0.0, 0.0, 32.0, 32.0]);
    scene.assign(moved, "translate_x", 32.0).unwrap();
    scene.assign(moved, "translate_y", 32.0).unwrap();
    let owner = node(&mut scene, moved, Element::Rect, [0.0, 0.0, 32.0, 32.0]);
    scene.assign(owner, "color", "#0000ffff").unwrap();
    let dot = scene.create(Element::Rect);
    scene.assign(dot, "radius", 16.0).unwrap();
    scene.set_mask(owner, Some(dot)).unwrap();
    let pixels = draw(&scene, root);
    assert_eq!(at(&pixels, 48, 48), [0, 0, 255, 255], "the moved centre");
    assert_eq!(at(&pixels, 34, 34)[3], 0, "the moved corner is cut");
    assert_eq!(at(&pixels, 16, 16)[3], 0, "nothing where it was");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_masked_shadow_is_cut_with_the_layer() {
    let (mut scene, root, square) = green_square();
    scene.assign(square, "color", "#00000000").unwrap();
    let card = node(&mut scene, root, Element::Rect, [8.0, 8.0, 32.0, 32.0]);
    scene.assign(card, "color", "#ff0000ff").unwrap();
    scene
        .assign(
            card,
            "layer",
            Value::Map(
                [
                    ("enabled".to_owned(), Value::Bool(true)),
                    (
                        "shadow_color".to_owned(),
                        Value::Color(Color::rgba8(0, 0, 255, 255)),
                    ),
                    ("shadow_offset_x".to_owned(), Value::Number(16.0)),
                    ("shadow_offset_y".to_owned(), Value::Number(16.0)),
                ]
                .into_iter()
                .collect(),
            ),
        )
        .unwrap();
    let unmasked = draw(&scene, root);
    assert_eq!(at(&unmasked, 50, 50)[2], 255, "the shadow, unmasked");
    // A mask the card's size, grown to reach the shadow's corner.
    let mask = scene.create(Element::Rect);
    scene.assign(mask, "width", 40.0).unwrap();
    scene.assign(mask, "height", 40.0).unwrap();
    scene.set_mask(card, Some(mask)).unwrap();
    let pixels = draw(&scene, root);
    assert_eq!(at(&pixels, 20, 20), [255, 0, 0, 255], "the card");
    assert_eq!(at(&pixels, 44, 44)[2], 255, "the shadow inside the mask");
    assert_eq!(at(&pixels, 52, 52)[3], 0, "the shadow past it is cut");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_hidden_mask_masks_nothing_and_an_empty_one_everything() {
    let (mut scene, root, square) = green_square();
    let circle = circle_mask(&mut scene, square);
    scene.assign(circle, "visible", false).unwrap();
    let pixels = draw(&scene, root);
    assert_eq!(at(&pixels, 2, 2), [0, 255, 0, 255], "hidden: no mask");
    scene.assign(circle, "visible", true).unwrap();
    scene.assign(circle, "color", "#ffffff00").unwrap();
    let pixels = draw(&scene, root);
    assert_eq!(at(&pixels, 32, 32)[3], 0, "transparent: everything masked");
}

/// Frames drawn through the damage each change leaves, against the same
/// frames drawn whole by a fresh backend.
#[test]
#[ignore = "requires a GPU adapter"]
fn an_animated_mask_damages_what_it_changes() {
    let (mut scene, root, square) = green_square();
    scene.assign(square, "color", "#202020ff").unwrap();
    // A list-like panel, masked by a moving dot, with a gradient-masked
    // strip inside it and a frosted pane over part of it.
    let panel = node(&mut scene, root, Element::Rect, [4.0, 4.0, 40.0, 50.0]);
    scene.assign(panel, "color", "#3a86ffff").unwrap();
    let stripe = node(&mut scene, panel, Element::Rect, [4.0, 10.0, 32.0, 8.0]);
    scene.assign(stripe, "color", "#ffbe0bff").unwrap();
    let dot = scene.create(Element::Rect);
    scene.assign(dot, "width", 24.0).unwrap();
    scene.assign(dot, "height", 24.0).unwrap();
    scene.assign(dot, "radius", 12.0).unwrap();
    scene.set_mask(panel, Some(dot)).unwrap();
    let strip = node(&mut scene, root, Element::Rect, [46.0, 4.0, 14.0, 56.0]);
    scene.assign(strip, "color", "#8338ecff").unwrap();
    let gradient = |middle: f64| {
        Value::Map(
            [(
                "gradient".to_owned(),
                Value::Map(
                    [(
                        "stops".to_owned(),
                        Value::List(vec![
                            Value::Number(0.0),
                            Value::List(vec![Value::Number(1.0), Value::Number(middle)]),
                            Value::Number(0.0),
                        ]),
                    )]
                    .into_iter()
                    .collect(),
                ),
            )]
            .into_iter()
            .collect(),
        )
    };
    scene.assign(strip, "mask", gradient(0.5)).unwrap();
    let pane = node(&mut scene, root, Element::Rect, [30.0, 30.0, 30.0, 30.0]);
    scene.assign(pane, "color", "#ffffff30").unwrap();
    scene.assign(pane, "backdrop_blur", 4.0).unwrap();
    let steps: Vec<(NodeHandle, &str, Value)> = vec![
        (dot, "x", Value::Number(12.0)),
        (dot, "y", Value::Number(20.0)),
        (dot, "radius", Value::Number(4.0)),
        (panel, "mask_invert", Value::Bool(true)),
        (strip, "mask", gradient(0.2)),
        (stripe, "x", Value::Number(10.0)),
        (panel, "opacity", Value::Number(0.7)),
        (dot, "visible", Value::Bool(false)),
        (dot, "visible", Value::Bool(true)),
        (panel, "mask_invert", Value::Bool(false)),
    ];
    let logical = Size {
        width: f64::from(SIZE),
        height: f64::from(SIZE),
    };
    let mut engine = RenderEngine::new(pollster::block_on(WgpuBackend::new(SIZE, SIZE)).unwrap());
    let layout = Layout::compute(&scene, root, logical, &mut NoText).unwrap();
    engine.render(&scene, &layout, 120, |_| {}).unwrap();
    for (step, (target, property, value)) in steps.into_iter().enumerate() {
        scene.assign(target, property, value).unwrap();
        let layout = Layout::compute(&scene, root, logical, &mut NoText).unwrap();
        let damage = engine.render(&scene, &layout, 120, |_| {}).unwrap();
        assert!(
            !damage.is_empty(),
            "step {step} ({property}) damaged nothing"
        );
        let partial = engine.backend_mut().read_pixels();
        let mut fresh =
            RenderEngine::new(pollster::block_on(WgpuBackend::new(SIZE, SIZE)).unwrap());
        fresh.render(&scene, &layout, 120, |_| {}).unwrap();
        let whole = fresh.backend_mut().read_pixels();
        let (worst, index) = partial
            .iter()
            .zip(&whole)
            .enumerate()
            .map(|(index, (a, b))| (a.abs_diff(*b), index))
            .max()
            .unwrap_or((0, 0));
        let pixel = index / 4;
        let (x, y) = (pixel as u32 % SIZE, pixel as u32 / SIZE);
        assert!(
            worst <= 2,
            "step {step} ({property}): the damaged frame differs from the whole one by \
             {worst} at ({x}, {y}): {:?} against {:?}, damage {damage:?}",
            &partial[pixel * 4..pixel * 4 + 4],
            &whole[pixel * 4..pixel * 4 + 4],
        );
    }
}
