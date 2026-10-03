//! A node playing its exit, on a real adapter: drawn while it leaves, over
//! the siblings that closed up under it, inside a rounded clip that is a
//! layer of its own -- and gone without a trace once it has left. Every
//! frame is drawn through the damage the tracker found and compared with
//! the same scene drawn whole by a fresh backend.

use std::time::Duration;

use morf_layout::{Layout, Size};
use morf_scene::{Behavior, Color, Element, ExitSpec, NodeHandle, Scene, Value};

use crate::tests::NoText;
use crate::*;

const SIZE: u32 = 96;
const AREA: Size = Size {
    width: SIZE as f64,
    height: SIZE as f64,
};

struct Stack {
    scene: Scene,
    root: NodeHandle,
    leaving: NodeHandle,
    last: NodeHandle,
}

fn bar(scene: &mut Scene, parent: NodeHandle, color: Color) -> NodeHandle {
    let node = scene.create(Element::Rect);
    scene.assign(node, "width", 72.0).unwrap();
    scene.assign(node, "height", 20.0).unwrap();
    scene.assign(node, "color", Value::Color(color)).unwrap();
    scene.reparent(node, Some(parent)).unwrap();
    node
}

/// Three bars in a column inside a rounded clip; the middle one, painted
/// over its siblings, leaves by fading and shrinking.
fn stack() -> Stack {
    let mut scene = Scene::new();
    let root = scene.create(Element::Rect);
    scene.assign(root, "width", AREA.width).unwrap();
    scene.assign(root, "height", AREA.height).unwrap();
    scene
        .assign(root, "color", Value::Color(Color::rgba8(20, 20, 20, 255)))
        .unwrap();
    let clip = scene.create(Element::ClipRect);
    for (property, value) in [("x", 4.0), ("y", 4.0), ("width", 88.0), ("height", 88.0)] {
        scene.assign(clip, property, value).unwrap();
    }
    scene.assign(clip, "radius", 14.0).unwrap();
    scene
        .assign(clip, "color", Value::Color(Color::rgba8(20, 20, 20, 255)))
        .unwrap();
    scene.reparent(clip, Some(root)).unwrap();
    let column = scene.create(Element::Column);
    for (property, value) in [("x", 8.0), ("y", 8.0), ("gap", 4.0)] {
        scene.assign(column, property, value).unwrap();
    }
    scene.reparent(column, Some(clip)).unwrap();
    bar(&mut scene, column, Color::rgba8(220, 40, 40, 255));
    let leaving = bar(&mut scene, column, Color::rgba8(40, 220, 40, 255));
    let last = bar(&mut scene, column, Color::rgba8(40, 40, 220, 255));
    scene.assign(leaving, "z", 1.0).unwrap();
    scene.assign(leaving, "radius", 6.0).unwrap();
    scene
        .set_exit(
            leaving,
            Some(ExitSpec {
                values: vec![
                    ("opacity".to_owned(), Value::Number(0.0)),
                    ("scale".to_owned(), Value::Number(0.5)),
                ],
                behavior: Behavior {
                    duration: Duration::from_millis(100),
                    ..Behavior::default()
                },
            }),
        )
        .unwrap();
    Stack {
        scene,
        root,
        leaving,
        last,
    }
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * SIZE + x) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

/// Draws the scene through the tracker's damage, and whole on a fresh
/// backend, and says how far apart the two came out.
fn draw(
    engine: &mut RenderEngine<WgpuBackend>,
    scene: &Scene,
    root: NodeHandle,
    layout: &mut Layout,
) -> Vec<u8> {
    layout.update(scene, root, AREA, &mut NoText).unwrap();
    engine.render(scene, layout, 120, |_| {}).unwrap();
    let drawn = engine.backend_mut().read_pixels();
    let mut fresh = RenderEngine::new(pollster::block_on(WgpuBackend::new(SIZE, SIZE)).unwrap());
    let whole_layout = Layout::compute(scene, root, AREA, &mut NoText).unwrap();
    fresh.render(scene, &whole_layout, 120, |_| {}).unwrap();
    let whole = fresh.backend_mut().read_pixels();
    let worst = drawn
        .iter()
        .zip(&whole)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    assert!(
        worst <= 1,
        "the damaged frame differs from the whole one by {worst}"
    );
    drawn
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_leaving_node_is_drawn_while_it_leaves_and_leaves_nothing_behind() {
    let Stack {
        mut scene,
        root,
        leaving,
        last,
    } = stack();
    let mut engine = RenderEngine::new(pollster::block_on(WgpuBackend::new(SIZE, SIZE)).unwrap());
    let mut layout = Layout::default();
    // The middle bar: x 12..84, y 36..56 on the surface.
    let (cx, cy) = (48, 46);
    let before = draw(&mut engine, &scene, root, &mut layout);
    assert_eq!(pixel(&before, cx, cy), [40, 220, 40, 255]);
    assert_eq!(pixel(&before, cx, 70), [40, 40, 220, 255], "the last bar");

    assert!(scene.begin_exit(leaving).unwrap());
    scene.tick_animations(Duration::from_millis(50)).unwrap();
    let midway = draw(&mut engine, &scene, root, &mut layout);
    let ghost = pixel(&midway, cx, cy);
    // Half of the green over the blue bar that closed up beneath it (mixed
    // in linear light, so each channel is well past the halfway byte).
    assert!(
        ghost[1] > 100 && ghost[1] < 190 && ghost[2] > 100 && ghost[2] < 190,
        "half faded, over blue: {ghost:?}"
    );
    assert_eq!(
        pixel(&midway, cx, 70),
        [20, 20, 20, 255],
        "the last bar has moved up out of its old place"
    );
    assert_eq!(
        pixel(&midway, 14, 38),
        [40, 40, 220, 255],
        "and the leaving bar, shrunk about its centre, no longer covers its corner"
    );
    assert_eq!(layout.geometry(last).unwrap().y, 36.0);

    let frame = scene.tick_animations(Duration::from_millis(60)).unwrap();
    assert_eq!(frame.exited, vec![leaving]);
    let faded = draw(&mut engine, &scene, root, &mut layout);
    assert_eq!(pixel(&faded, cx, cy), [40, 40, 220, 255], "faded out");
    scene.remove(leaving).unwrap();
    let after = draw(&mut engine, &scene, root, &mut layout);
    assert_eq!(
        pixel(&after, cx, cy),
        [40, 40, 220, 255],
        "gone, and nothing left"
    );
}
