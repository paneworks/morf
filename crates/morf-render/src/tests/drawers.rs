//! A panel sliding out of a screen-edge frame: a field layer tracking an
//! animated node, a stretch, and the damage the slide costs.

use std::time::Duration;

use morf_scene::{Behavior, Easing, Stretch};

use super::*;

const WIDTH: f64 = 800.0;
const HEIGHT: f64 = 600.0;
const SEAM: f64 = 16.0;

struct Drawer {
    scene: Scene,
    root: NodeHandle,
    panel: NodeHandle,
    background: NodeHandle,
}

/// A frame — the screen minus a rounded inner box — and a panel under its
/// top edge, tucked up inside the frame until it is opened.
fn drawer(stretch: bool) -> Drawer {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let field = scene.create(Element::Sdf);
    let outer = scene.create(Element::SdfShape);
    let inner = scene.create(Element::SdfShape);
    let background = scene.create(Element::SdfShape);
    let panel = scene.create(Element::Item);
    let fill = Value::Map([("fill".to_owned(), Value::Bool(true))].into());
    scene.assign(root, "width", WIDTH).unwrap();
    scene.assign(root, "height", HEIGHT).unwrap();
    scene.assign(field, "anchors", fill.clone()).unwrap();
    scene.assign(field, "blend", SEAM).unwrap();
    scene.assign(field, "blend_profile", "circular").unwrap();
    scene.assign(outer, "shape", "box").unwrap();
    scene.assign(outer, "anchors", fill).unwrap();
    scene.assign(inner, "shape", "box").unwrap();
    for (property, value) in [
        ("x", 20.0),
        ("y", 20.0),
        ("width", WIDTH - 40.0),
        ("height", HEIGHT - 40.0),
        ("radius", 24.0),
    ] {
        scene.assign(inner, property, value).unwrap();
    }
    scene.assign(inner, "operation", "subtract").unwrap();
    scene.assign(background, "shape", "box").unwrap();
    scene.assign(background, "radius", 16.0).unwrap();
    scene
        .assign(background, "operation", "smooth_union")
        .unwrap();
    scene.assign(background, "blend_group", 1.0).unwrap();
    scene.set_track(background, Some(panel)).unwrap();
    for (property, value) in [
        ("x", 300.0),
        ("y", 20.0),
        ("width", 200.0),
        ("height", 120.0),
        ("translate_y", -120.0),
    ] {
        scene.assign(panel, property, value).unwrap();
    }
    scene
        .set_behavior(
            panel,
            "translate_y",
            Some(Behavior::timed(
                Duration::from_millis(320),
                Easing::OutCubic,
            )),
        )
        .unwrap();
    if stretch {
        scene
            .set_stretch(
                panel,
                Some(Stretch {
                    scale: 0.2,
                    ..Stretch::default()
                }),
            )
            .unwrap();
    }
    for node in [outer, inner, background] {
        scene.reparent(node, Some(field)).unwrap();
    }
    scene.reparent(field, Some(root)).unwrap();
    scene.reparent(panel, Some(root)).unwrap();
    Drawer {
        scene,
        root,
        panel,
        background,
    }
}

impl Drawer {
    /// One frame: motion, layout, the stretch springs, and the draw list.
    fn frame(&mut self) -> (Layout, DrawList) {
        self.scene
            .tick_animations(Duration::from_millis(16))
            .unwrap();
        let layout = Layout::compute(
            &self.scene,
            self.root,
            Size {
                width: WIDTH,
                height: HEIGHT,
            },
            &mut NoText,
        )
        .unwrap();
        morf_layout::observe_stretch(&mut self.scene, &layout).unwrap();
        let list = DrawList::from_scene(&self.scene, &layout).unwrap();
        (layout, list)
    }
}

fn field_layers(list: &DrawList) -> &[SdfLayer] {
    list.commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::Field { layers, .. } => Some(layers.as_slice()),
            _ => None,
        })
        .expect("the frame is a field")
}

/// Where the panel is drawn this frame, through every transform.
fn drawn(drawer: &Drawer, layout: &Layout) -> Geometry {
    let geometry = layout.geometry(drawer.panel).unwrap();
    layout
        .chain_transform(&drawer.scene, drawer.panel)
        .unwrap()
        .bounds(geometry)
}

fn close(left: Geometry, right: Geometry) -> bool {
    [
        (left.x, right.x),
        (left.y, right.y),
        (left.width, right.width),
        (left.height, right.height),
    ]
    .iter()
    .all(|(a, b)| (a - b).abs() < 1e-3)
}

#[test]
fn a_tracking_layer_is_exactly_where_its_animated_node_is_drawn() {
    let mut drawer = drawer(false);
    let (layout, list) = drawer.frame();
    assert_eq!(field_layers(&list).len(), 3);
    assert!(close(
        field_layers(&list)[2].bounds,
        drawn(&drawer, &layout)
    ));
    drawer
        .scene
        .assign(drawer.panel, "translate_y", 0.0)
        .unwrap();
    let mut seen = Vec::new();
    for _ in 0..24 {
        let (layout, list) = drawer.frame();
        let layer = field_layers(&list)[2].bounds;
        assert!(
            close(layer, drawn(&drawer, &layout)),
            "{layer:?} against {:?}",
            drawn(&drawer, &layout)
        );
        seen.push(layer.y);
    }
    // It moved, all the way, and the layer went with it every frame.
    assert!(seen.windows(2).any(|pair| pair[1] > pair[0]));
    assert!((seen.last().unwrap() - 20.0).abs() < 1e-9);
    // Its group and seam came through for the shader.
    let list = list_of(&mut drawer);
    let layer = &field_layers(&list)[2];
    assert_eq!(layer.blend_group, 1);
    assert_eq!(layer.profile, BlendProfile::Circular);
    assert_eq!(layer.matrix, [1.0, 0.0, 0.0, 1.0]);
}

fn list_of(drawer: &mut Drawer) -> DrawList {
    drawer.frame().1
}

#[test]
fn a_tracking_layer_follows_a_stretch_as_an_exact_box() {
    let mut drawer = drawer(true);
    drawer.frame();
    drawer
        .scene
        .assign(drawer.panel, "translate_y", 0.0)
        .unwrap();
    let mut stretched = false;
    for _ in 0..10 {
        let (layout, list) = drawer.frame();
        let layer = &field_layers(&list)[2];
        assert!(close(layer.bounds, drawn(&drawer, &layout)));
        // A stretch straight down is along the axes: it becomes the layer's
        // size, so the corners stay round rather than going elliptical.
        assert_eq!(layer.matrix, [1.0, 0.0, 0.0, 1.0]);
        if layer.bounds.height > 120.5 {
            stretched = true;
            assert!(layer.bounds.width < 200.0, "narrower as it lengthens");
            assert!(layer.radii[0] < 16.0 && layer.radii[0] > 12.0);
        }
    }
    assert!(stretched, "sliding stretched the panel and its layer");
}

#[test]
fn a_hidden_tracked_node_takes_its_layer_with_it() {
    let mut drawer = drawer(false);
    drawer.scene.assign(drawer.panel, "visible", false).unwrap();
    let (_, list) = drawer.frame();
    assert_eq!(field_layers(&list).len(), 2);
}

#[test]
fn sliding_a_panel_damages_the_panel_and_not_the_frame() {
    let mut drawer = drawer(true);
    let mut tracker = DamageTracker::default();
    let (_, mut list) = drawer.frame();
    let first = tracker.diff(&list, 120);
    tracker.retain(&mut list);
    let full: u64 = first
        .iter()
        .map(|rect| u64::from(rect.width) * u64::from(rect.height))
        .sum();
    assert!(
        full >= (WIDTH * HEIGHT) as u64,
        "the first frame is all of it"
    );

    drawer
        .scene
        .assign(drawer.panel, "translate_y", 0.0)
        .unwrap();
    let mut before = drawn(&drawer, &{
        Layout::compute(
            &drawer.scene,
            drawer.root,
            Size {
                width: WIDTH,
                height: HEIGHT,
            },
            &mut NoText,
        )
        .unwrap()
    });
    let mut most = 0u64;
    for _ in 0..40 {
        let (layout, mut list) = drawer.frame();
        let now = drawn(&drawer, &layout);
        let damage = tracker.diff(&list, 120);
        tracker.retain(&mut list);
        // Everything damaged is within reach of where the panel was or is:
        // its box, the seam either side, the fillet along the frame.
        let reach = expand_geometry(union_geometry(before, now), SEAM * 2.5 + 4.0);
        for rect in &damage {
            let inside = f64::from(rect.x) >= reach.x.floor()
                && f64::from(rect.y) >= reach.y.floor()
                && f64::from(rect.x + rect.width) <= (reach.x + reach.width).ceil()
                && f64::from(rect.y + rect.height) <= (reach.y + reach.height).ceil();
            assert!(inside, "{rect:?} is outside the panel's reach {reach:?}");
        }
        let area: u64 = damage
            .iter()
            .map(|rect| u64::from(rect.width) * u64::from(rect.height))
            .sum();
        most = most.max(area);
        before = now;
    }
    assert!(
        most * 5 < full,
        "a frame of the slide damaged {most} of {full} pixels"
    );
    // Once it has settled, nothing.
    for _ in 0..80 {
        let (_, mut list) = drawer.frame();
        let _ = tracker.diff(&list, 120);
        tracker.retain(&mut list);
    }
    let (_, list) = drawer.frame();
    assert!(tracker.diff(&list, 120).is_empty(), "a still frame is free");
    assert!(!drawer.scene.has_motion(), "and asks for no more frames");
}

#[test]
fn a_layer_s_opacity_reaches_its_field_and_nothing_else() {
    let mut drawer = drawer(false);
    drawer
        .scene
        .assign(drawer.background, "opacity", 0.4)
        .unwrap();
    let (_, list) = drawer.frame();
    let layers = field_layers(&list);
    assert!((layers[2].opacity - 0.4).abs() < 1e-6);
    assert_eq!(layers[0].opacity, 1.0);
    // Faded by the field as one of its layers: no offscreen target for a
    // shape that paints nothing of its own.
    assert!(list.layers.is_empty(), "{:?}", list.layers);
}

#[test]
fn fading_a_layer_damages_the_layer_and_not_the_frame() {
    let mut drawer = drawer(false);
    drawer
        .scene
        .assign(drawer.panel, "translate_y", 0.0)
        .unwrap();
    drawer
        .scene
        .assign(drawer.background, "opacity", 0.0)
        .unwrap();
    for _ in 0..40 {
        drawer.frame();
    }
    let mut tracker = DamageTracker::default();
    let (layout, mut list) = drawer.frame();
    let full: u64 = tracker
        .diff(&list, 120)
        .iter()
        .map(|rect| u64::from(rect.width) * u64::from(rect.height))
        .sum();
    tracker.retain(&mut list);
    let panel = drawn(&drawer, &layout);
    drawer
        .scene
        .set_behavior(
            drawer.background,
            "opacity",
            Some(Behavior::timed(Duration::from_millis(160), Easing::Linear)),
        )
        .unwrap();
    drawer
        .scene
        .assign(drawer.background, "opacity", 1.0)
        .unwrap();
    let mut faded = Vec::new();
    for _ in 0..14 {
        let (_, mut list) = drawer.frame();
        let opacity = field_layers(&list)[2].opacity;
        let moved = faded.last() != Some(&opacity);
        faded.push(opacity);
        let damage = tracker.diff(&list, 120);
        tracker.retain(&mut list);
        assert_eq!(!damage.is_empty(), moved, "a frame of the fade repaints");
        let reach = expand_geometry(panel, SEAM * 2.5 + 4.0);
        let area: u64 = damage
            .iter()
            .map(|rect| u64::from(rect.width) * u64::from(rect.height))
            .sum();
        for rect in &damage {
            let inside = f64::from(rect.x) >= reach.x.floor()
                && f64::from(rect.y) >= reach.y.floor()
                && f64::from(rect.x + rect.width) <= (reach.x + reach.width).ceil()
                && f64::from(rect.y + rect.height) <= (reach.y + reach.height).ceil();
            assert!(inside, "{rect:?} is outside the panel's reach {reach:?}");
        }
        assert!(
            area * 5 < full,
            "a frame of the fade damaged {area} of {full}"
        );
    }
    // It animated, through the values in between, to whole.
    assert!(faded.iter().any(|opacity| *opacity > 0.2 && *opacity < 0.8));
    assert!(faded.windows(2).all(|pair| pair[1] >= pair[0]));
    assert_eq!(*faded.last().unwrap(), 1.0);
}
