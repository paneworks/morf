use crate::*;

// A panel growing out of a screen-edge frame: the circular seam, blend
// groups, a layer's own matrix, and a layer tracking a node, drawn.

use crate::gpu::field_tests::{alpha_at, field_command, field_layer, render_readback};
use crate::{BlendProfile, Operation, Shape};

const SIZE: u32 = 128;
const EDGE: f64 = 8.0;

/// A frame eight pixels thick round a `SIZE` square, and panels hanging from
/// its top edge, each `(left, width, group)`.
fn frame_with(panels: &[(f64, f64, u32)], profile: BlendProfile, blend: f32) -> DrawList {
    let mut scene = morf_scene::Scene::new();
    let node = scene.create(morf_scene::Element::Sdf);
    let whole = field_layer(0.0, 0.0, f64::from(SIZE), Shape::Box);
    let mut hole = field_layer(EDGE, EDGE, f64::from(SIZE) - EDGE * 2.0, Shape::Box);
    hole.operation = Operation::Subtract;
    let mut layers = vec![whole, hole];
    for &(left, width, group) in panels {
        let mut panel = field_layer(left, EDGE, width, Shape::Box);
        panel.bounds.height = 40.0;
        panel.operation = if blend > 0.0 {
            Operation::SmoothUnion
        } else {
            Operation::Union
        };
        panel.blend = blend;
        panel.profile = profile;
        panel.blend_group = group;
        layers.push(panel);
    }
    let mut command = field_command(node, layers);
    if let DrawCommand::Field { bounds, .. } = &mut command {
        bounds.width = f64::from(SIZE);
        bounds.height = f64::from(SIZE);
    }
    DrawList {
        commands: vec![command],
        layers: Vec::new(),
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_panel_merges_into_the_frame_through_a_quarter_circle() {
    // The panel's left side meets the frame's inner edge at (40, 8). A
    // circular seam of 12 fills that inside corner up to the arc centred at
    // (28, 20): just inside the arc is filled, just outside it is not.
    let circular = render_readback(
        &frame_with(&[(40.0, 48.0, 0)], BlendProfile::Circular, 12.0),
        SIZE,
    );
    let hard = render_readback(
        &frame_with(&[(40.0, 48.0, 0)], BlendProfile::Circular, 0.0),
        SIZE,
    );
    assert_eq!(
        alpha_at(&hard, SIZE, 37, 10),
        0,
        "a hard union leaves the corner square"
    );
    assert_eq!(
        alpha_at(&circular, SIZE, 37, 10),
        255,
        "the fillet fills the corner"
    );
    assert_eq!(alpha_at(&circular, SIZE, 35, 12), 0, "and stops at the arc");
    // And on the right side, the mirror image.
    assert_eq!(alpha_at(&circular, SIZE, 90, 10), 255);
    assert_eq!(alpha_at(&circular, SIZE, 92, 12), 0);
    // Outside the seam the frame and the panel are exactly themselves.
    assert_eq!(
        alpha_at(&circular, SIZE, 12, 12),
        0,
        "the frame's inside edge"
    );
    assert_eq!(alpha_at(&circular, SIZE, 12, 6), 255, "the frame");
    assert_eq!(alpha_at(&circular, SIZE, 64, 46), 255, "the panel");
    assert_eq!(alpha_at(&circular, SIZE, 64, 50), 0, "below the panel");
    assert_eq!(alpha_at(&circular, SIZE, 64, 100), 0, "the middle is open");
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn panels_in_different_blend_groups_do_not_bridge() {
    // Two panels six pixels apart with a seam of twelve: in one group the gap
    // fills; in two groups it stays open, while both still merge with the
    // frame they hang from.
    let panels = |left_group, right_group| {
        frame_with(
            &[(20.0, 40.0, left_group), (66.0, 40.0, right_group)],
            BlendProfile::Quadratic,
            12.0,
        )
    };
    let together = render_readback(&panels(1, 1), SIZE);
    let apart = render_readback(&panels(1, 2), SIZE);
    assert!(
        alpha_at(&together, SIZE, 63, 40) > 128,
        "one group bridges the gap"
    );
    assert_eq!(alpha_at(&apart, SIZE, 63, 40), 0, "two groups meet hard");
    for pixels in [&together, &apart] {
        assert_eq!(
            alpha_at(pixels, SIZE, 18, 9),
            255,
            "each still merges into the frame"
        );
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_layer_matrix_shears_its_shape() {
    let mut scene = morf_scene::Scene::new();
    let node = scene.create(morf_scene::Element::Sdf);
    // A 40 by 20 box centred at (64, 64), sheared so each pixel down moves it
    // one pixel right.
    let mut layer = field_layer(44.0, 54.0, 40.0, Shape::Box);
    layer.bounds.height = 20.0;
    layer.matrix = [1.0, 0.0, 1.0, 1.0];
    let mut command = field_command(node, vec![layer]);
    if let DrawCommand::Field { bounds, .. } = &mut command {
        bounds.width = f64::from(SIZE);
        bounds.height = f64::from(SIZE);
    }
    let pixels = render_readback(
        &DrawList {
            commands: vec![command],
            layers: Vec::new(),
        },
        SIZE,
    );
    // Bottom row, 8 below the centre: the box runs from 52 to 92.
    assert_eq!(
        alpha_at(&pixels, SIZE, 90, 72),
        255,
        "the bottom leans right"
    );
    assert_eq!(alpha_at(&pixels, SIZE, 48, 72), 0);
    // Top row, 8 above: from 36 to 76.
    assert_eq!(alpha_at(&pixels, SIZE, 38, 56), 255, "the top leans left");
    assert_eq!(alpha_at(&pixels, SIZE, 80, 56), 0);
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_frame_and_a_tracked_sliding_panel_render_from_the_scene() {
    use morf_scene::{Element, Scene, Value};
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let field = scene.create(Element::Sdf);
    let outer = scene.create(Element::SdfShape);
    let inner = scene.create(Element::SdfShape);
    let background = scene.create(Element::SdfShape);
    let panel = scene.create(Element::Item);
    let fill = Value::Map([("fill".to_owned(), Value::Bool(true))].into());
    scene.assign(root, "width", f64::from(SIZE)).unwrap();
    scene.assign(root, "height", f64::from(SIZE)).unwrap();
    scene.assign(field, "anchors", fill.clone()).unwrap();
    scene.assign(field, "blend", 10.0).unwrap();
    scene.assign(field, "blend_profile", "circular").unwrap();
    scene.assign(outer, "shape", "box").unwrap();
    scene.assign(outer, "anchors", fill).unwrap();
    scene.assign(inner, "shape", "box").unwrap();
    for (property, value) in [("x", 8.0), ("y", 8.0), ("width", 112.0), ("height", 112.0)] {
        scene.assign(inner, property, value).unwrap();
    }
    scene.assign(inner, "operation", "subtract").unwrap();
    scene.assign(background, "shape", "box").unwrap();
    scene
        .assign(background, "operation", "smooth_union")
        .unwrap();
    scene.set_track(background, Some(panel)).unwrap();
    // Laid out tucked into the frame, and moved half way out by its
    // transform alone: the layer has to follow the transform, not the layout.
    for (property, value) in [
        ("x", 40.0),
        ("y", -32.0),
        ("width", 48.0),
        ("height", 40.0),
        ("translate_y", 20.0),
    ] {
        scene.assign(panel, property, value).unwrap();
    }
    for node in [outer, inner, background] {
        scene.reparent(node, Some(field)).unwrap();
    }
    scene.reparent(field, Some(root)).unwrap();
    scene.reparent(panel, Some(root)).unwrap();
    let layout = morf_layout::Layout::compute(
        &scene,
        root,
        morf_layout::Size {
            width: f64::from(SIZE),
            height: f64::from(SIZE),
        },
        &mut crate::tests::NoText,
    )
    .unwrap();
    let list = DrawList::from_scene(&scene, &layout).unwrap();
    let pixels = render_readback(&list, SIZE);
    // The panel reaches from -12 to 28 after its move.
    assert_eq!(
        alpha_at(&pixels, SIZE, 64, 25),
        255,
        "the panel is where it was moved"
    );
    assert_eq!(alpha_at(&pixels, SIZE, 64, 31), 0, "and no further");
    assert_eq!(
        alpha_at(&pixels, SIZE, 38, 9),
        255,
        "merged into the frame by a fillet"
    );
    assert_eq!(
        alpha_at(&pixels, SIZE, 20, 20),
        0,
        "the inside is open elsewhere"
    );
}

/// A 512 frame with panels on two edges in two groups, one of them sheared:
/// large enough to be drawn as tiles.
fn large_frame() -> (DrawList, Vec<SdfLayer>) {
    let size = 512.0;
    let mut scene = morf_scene::Scene::new();
    let node = scene.create(morf_scene::Element::Sdf);
    let whole = field_layer(0.0, 0.0, size, Shape::Box);
    let mut hole = field_layer(12.0, 12.0, size - 24.0, Shape::Box);
    hole.operation = Operation::Subtract;
    hole.radii = [24.0; 4];
    let mut layers = vec![whole, hole];
    for (index, (x, y, w, h)) in [(180.0, 12.0, 150.0, 90.0), (12.0, 200.0, 80.0, 140.0)]
        .into_iter()
        .enumerate()
    {
        let mut panel = field_layer(x, y, w, Shape::Box);
        panel.bounds.height = h;
        panel.radii = [14.0; 4];
        panel.operation = Operation::SmoothUnion;
        panel.blend = 16.0;
        panel.profile = BlendProfile::Circular;
        panel.blend_group = index as u32 + 1;
        layers.push(panel);
    }
    layers[3].matrix = [1.0, 0.1, 0.0, 1.0];
    let mut command = field_command(node, layers.clone());
    if let DrawCommand::Field { bounds, .. } = &mut command {
        bounds.width = size;
        bounds.height = size;
    }
    (
        DrawList {
            commands: vec![command],
            layers: Vec::new(),
        },
        layers,
    )
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_tiled_frame_paints_exactly_where_its_cpu_twin_says() {
    let (list, layers) = large_frame();
    // It is tiled: most of the inside is not drawn at all.
    let tiles = crate::field_tiles(
        &layers,
        morf_layout::Geometry {
            x: 0.0,
            y: 0.0,
            width: 512.0,
            height: 512.0,
        },
        [0.0, 0.0, 512.0, 512.0],
        1.0,
        crate::Spill {
            edge: 2.0,
            shadow: None,
            solid: true,
        },
    )
    .expect("a large hollow frame is tiled");
    let drawn: f32 = tiles
        .iter()
        .map(|tile| (tile.area[2] - tile.area[0]) * (tile.area[3] - tile.area[1]))
        .sum();
    assert!(drawn < 512.0 * 512.0 * 0.75, "{drawn} of the quad drawn");
    let pixels = render_readback(&list, 512);
    let mut checked = 0;
    for y in 0..512 {
        for x in 0..512 {
            let distance = composed_distance(&layers, [x as f32 + 0.5, y as f32 + 0.5]);
            let alpha = alpha_at(&pixels, 512, x, y);
            if distance < -1.5 {
                assert_eq!(alpha, 255, "inside at {x},{y} ({distance})");
                checked += 1;
            } else if distance > 1.5 {
                assert_eq!(alpha, 0, "outside at {x},{y} ({distance})");
                checked += 1;
            }
        }
    }
    assert!(checked > 200_000, "{checked} pixels decided");
}
