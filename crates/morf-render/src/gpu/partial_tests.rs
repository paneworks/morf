//! Layers rendered only over what a frame reads of them, on a real adapter:
//! a frame drawn through small damage must come out as the same frame drawn
//! whole from nothing.
//!
//! Every kind of layer is on the desk — rounded clips inside rounded clips, a
//! bordered clip, a turned child, an opacity group around everything, a
//! blurred and a shadowed layer, frosted glass — and each step changes one
//! small thing and compares the surface against a fresh backend that drew the
//! same scene in one go. Run at a whole and a fractional scale.

use morf_layout::{Layout, Size};
use morf_scene::{Color, Element, NodeHandle, Scene, Value};

use crate::tests::NoText;
use crate::*;

const LOGICAL: f64 = 96.0;

struct Desk {
    scene: Scene,
    root: NodeHandle,
    /// Things to change between frames, and what to set them to.
    steps: Vec<(NodeHandle, &'static str, Value)>,
}

fn rect(scene: &mut Scene, parent: NodeHandle, element: Element, place: [f64; 4]) -> NodeHandle {
    let node = scene.create(element);
    for (property, value) in ["x", "y", "width", "height"].into_iter().zip(place) {
        scene.assign(node, property, value).unwrap();
    }
    scene.reparent(node, Some(parent)).unwrap();
    node
}

fn desk() -> Desk {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", LOGICAL).unwrap();
    scene.assign(root, "height", LOGICAL).unwrap();
    // Everything under one fading group, as a desk fading in is.
    let group = rect(
        &mut scene,
        root,
        Element::Item,
        [0.0, 0.0, LOGICAL, LOGICAL],
    );
    scene.assign(group, "opacity", 0.97).unwrap();
    let ground = rect(
        &mut scene,
        group,
        Element::Rect,
        [0.0, 0.0, LOGICAL, LOGICAL],
    );
    let gradient = Value::Map(
        [
            ("kind".to_owned(), Value::String("linear".to_owned())),
            ("angle".to_owned(), Value::Number(35.0)),
            (
                "stops".to_owned(),
                Value::List(vec![
                    Value::String("#1d3557".to_owned()),
                    Value::String("#e63946".to_owned()),
                    Value::String("#f1faee".to_owned()),
                ]),
            ),
        ]
        .into_iter()
        .collect(),
    );
    scene.assign(ground, "gradient", gradient).unwrap();
    // A bordered rounded panel, a rounded clip inside it, and a turned bar
    // inside that.
    let panel = rect(&mut scene, group, Element::ClipRect, [6.0, 6.0, 50.0, 40.0]);
    scene.assign(panel, "radius", 9.0).unwrap();
    scene.assign(panel, "color", "#20202acc").unwrap();
    scene.assign(panel, "border_width", 2.0).unwrap();
    scene.assign(panel, "border_color", "#ffffff80").unwrap();
    let inner = rect(&mut scene, panel, Element::ClipRect, [4.0, 4.0, 30.0, 24.0]);
    scene.assign(inner, "radius", 6.0).unwrap();
    scene.assign(inner, "color", "#3a86ff").unwrap();
    let hand = rect(&mut scene, inner, Element::Rect, [12.0, 2.0, 3.0, 18.0]);
    scene.assign(hand, "color", "#ffbe0b").unwrap();
    scene.assign(hand, "rotation", 20.0).unwrap();
    // A frosted pane over the ground, with a dot on it.
    let pane = rect(
        &mut scene,
        group,
        Element::ClipRect,
        [58.0, 8.0, 32.0, 30.0],
    );
    scene.assign(pane, "radius", 7.0).unwrap();
    scene.assign(pane, "color", "#ffffff30").unwrap();
    scene.assign(pane, "backdrop_blur", 6.0).unwrap();
    let dot = rect(&mut scene, pane, Element::Rect, [4.0, 4.0, 6.0, 6.0]);
    scene.assign(dot, "color", "#06d6a0").unwrap();
    // A card with a layer shadow, and a blurred blob.
    let card = rect(&mut scene, group, Element::Rect, [10.0, 56.0, 34.0, 26.0]);
    scene.assign(card, "color", "#8338ec").unwrap();
    scene.assign(card, "radius", 5.0).unwrap();
    scene
        .assign(
            card,
            "layer",
            Value::Map(
                [
                    ("enabled".to_owned(), Value::Bool(true)),
                    (
                        "shadow_color".to_owned(),
                        Value::Color(Color::rgba8(0, 0, 0, 160)),
                    ),
                    ("shadow_blur".to_owned(), Value::Number(6.0)),
                    ("shadow_offset_x".to_owned(), Value::Number(3.0)),
                    ("shadow_offset_y".to_owned(), Value::Number(4.0)),
                ]
                .into_iter()
                .collect(),
            ),
        )
        .unwrap();
    let label = rect(&mut scene, card, Element::Rect, [4.0, 4.0, 10.0, 4.0]);
    scene.assign(label, "color", "#ffffff").unwrap();
    let blob = rect(&mut scene, group, Element::Rect, [60.0, 58.0, 22.0, 22.0]);
    scene.assign(blob, "color", "#fb5607").unwrap();
    scene.assign(blob, "blur", 3.0).unwrap();
    let steps = vec![
        (hand, "rotation", Value::Number(55.0)),
        (hand, "rotation", Value::Number(-30.0)),
        (dot, "x", Value::Number(14.0)),
        (label, "width", Value::Number(18.0)),
        (inner, "opacity", Value::Number(0.6)),
        (ground, "x", Value::Number(1.0)),
        (blob, "y", Value::Number(62.0)),
        (card, "x", Value::Number(14.0)),
        (panel, "y", Value::Number(9.0)),
        (inner, "opacity", Value::Number(1.0)),
    ];
    Desk { scene, root, steps }
}

fn frame(
    engine: &mut RenderEngine<WgpuBackend>,
    scene: &Scene,
    root: NodeHandle,
    scale_120: u32,
) -> Vec<DamageRect> {
    let logical = Size {
        width: LOGICAL,
        height: LOGICAL,
    };
    let layout = Layout::compute(scene, root, logical, &mut NoText).unwrap();
    engine.render(scene, &layout, scale_120, |_| {}).unwrap()
}

fn largest_difference(left: &[u8], right: &[u8], width: u32) -> (u8, u32, u32) {
    let mut worst = (0, 0, 0);
    for (index, (a, b)) in left.iter().zip(right).enumerate() {
        let difference = a.abs_diff(*b);
        if difference > worst.0 {
            let pixel = index as u32 / 4;
            worst = (difference, pixel % width, pixel / width);
        }
    }
    worst
}

#[test]
#[ignore = "requires a GPU adapter"]
fn frames_drawn_through_damage_match_frames_drawn_whole() {
    for scale_120 in [120, 180] {
        let physical = (LOGICAL * f64::from(scale_120) / 120.0).round() as u32;
        let mut desk = desk();
        let mut engine =
            RenderEngine::new(pollster::block_on(WgpuBackend::new(physical, physical)).unwrap());
        frame(&mut engine, &desk.scene, desk.root, scale_120);
        for (step, (node, property, value)) in
            std::mem::take(&mut desk.steps).into_iter().enumerate()
        {
            desk.scene.assign(node, property, value).unwrap();
            let damage = frame(&mut engine, &desk.scene, desk.root, scale_120);
            let drawn = engine.backend_mut().read_pixels();

            let mut fresh = RenderEngine::new(
                pollster::block_on(WgpuBackend::new(physical, physical)).unwrap(),
            );
            frame(&mut fresh, &desk.scene, desk.root, scale_120);
            let whole = fresh.backend_mut().read_pixels();

            let (difference, x, y) = largest_difference(&drawn, &whole, physical);
            // A step of one allows for the composite of a layer landing at a
            // different place in a differently sized texture.
            assert!(
                difference <= 1,
                "scale {scale_120}, step {step} ({property}): the damaged frame differs from \
                 the whole one by {difference} at ({x}, {y}): {:?} against {:?}, damage {damage:?}",
                &drawn[((y * physical + x) * 4) as usize..][..4],
                &whole[((y * physical + x) * 4) as usize..][..4],
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn layers_take_targets_for_what_is_read_of_them_and_nothing_else() {
    let desk = desk();
    let physical = LOGICAL as u32;
    let layout = Layout::compute(
        &desk.scene,
        desk.root,
        Size {
            width: LOGICAL,
            height: LOGICAL,
        },
        &mut NoText,
    )
    .unwrap();
    let list = DrawList::from_scene(&desk.scene, &layout).unwrap();
    let mut backend = pollster::block_on(WgpuBackend::new(physical, physical)).unwrap();
    // Nothing damaged: no layer is rendered, so none takes a target. (The
    // first frame blurs the glass, which reads what is beneath it.)
    let whole = DamageRect {
        x: 0,
        y: 0,
        width: physical,
        height: physical,
    };
    backend.render(&list, &[whole], 120).unwrap();
    backend.layer_pool.clear();
    backend.render(&list, &[], 120).unwrap();
    assert_eq!(
        backend.layer_pool.footprint(),
        (0, 0),
        "nothing read, nothing taken"
    );
    // A few pixels inside the inner clip: the layers holding them are drawn
    // there, and together they take far less than the target per layer, the
    // size of the surface, they used to.
    let damage = DamageRect {
        x: 20,
        y: 16,
        width: 6,
        height: 6,
    };
    backend.layer_pool.clear();
    backend.render(&list, &[damage], 120).unwrap();
    let (_, pixels) = backend.layer_pool.footprint();
    let surface = u64::from(physical * physical);
    assert!(
        pixels < list.layers.len() as u64 * surface / 2,
        "{} layers took {pixels} pixels of targets for a 6x6 damage",
        list.layers.len()
    );
}
