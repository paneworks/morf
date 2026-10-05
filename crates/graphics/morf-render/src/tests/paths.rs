//! `Path`: its outline as geometry, and the pixels it becomes.

use kurbo::{BezPath, ParamCurveArclen, PathEl, Point};
use morf_layout::{Layout, Size};
use morf_scene::{Color, Element, Scene, Value};

use super::NoText;
use crate::commands::*;
use crate::path::*;

fn length(path: &BezPath) -> f64 {
    path.segments().map(|segment| segment.arclen(1e-6)).sum()
}

fn paint(d: &str) -> PathPaint {
    PathPaint {
        d: d.to_owned(),
        morph_to: String::new(),
        morph_progress: 0.0,
        fill_color: Color::rgba8(255, 0, 0, 255),
        fill_rule: morf_scene::FillRule::NonZero,
        stroke_color: Color::rgba8(0, 0, 0, 0),
        stroke_width: 1.0,
        stroke_cap: morf_scene::StrokeCap::Butt,
        stroke_join: morf_scene::StrokeJoin::Miter,
        miter_limit: 4.0,
        dash: Vec::new(),
        dash_offset: 0.0,
        trim_start: 0.0,
        trim_end: 1.0,
        view_box: None,
        fill_mode: ImageFillMode::Stretch,
    }
}

#[test]
fn a_trim_keeps_that_fraction_of_the_whole_length() {
    // Two strokes, one after another: 100 long and 50 long.
    let path = BezPath::from_svg("M0 0 L100 0 M0 10 L50 10").unwrap();
    let half = trim(&path, 0.0, 0.5);
    assert!((length(&half) - 75.0).abs() < 1e-6, "{}", length(&half));
    // Across the gap between the two, it starts a second subpath rather than
    // drawing a line through the gap.
    let middle = trim(&path, 0.5, 0.9);
    let moves = middle
        .iter()
        .filter(|element| matches!(element, PathEl::MoveTo(_)))
        .count();
    assert_eq!(moves, 2);
    assert!((length(&middle) - 60.0).abs() < 1e-6);
    // Nothing when the ends cross, everything when they span it.
    assert!(trim(&path, 0.7, 0.3).elements().is_empty());
    assert_eq!(trim(&path, 0.0, 1.0), path);
}

#[test]
fn a_morph_walks_points_when_the_segments_line_up() {
    let smile = BezPath::from_svg("M0 0 C10 10 20 10 30 0").unwrap();
    let frown = BezPath::from_svg("M0 10 C10 0 20 0 30 10").unwrap();
    let halfway = morph(&smile, &frown, 0.5);
    let PathEl::MoveTo(start) = halfway.elements()[0] else {
        panic!("a morph starts where both do");
    };
    assert_eq!(start, Point::new(0.0, 5.0));
    // A line meets a curve: it counts as the cubic along it.
    let line = BezPath::from_svg("M0 0 L30 0").unwrap();
    let bent = morph(&line, &smile, 0.5);
    assert_eq!(bent.elements().len(), 2);
    // Different runs of segments have nothing to walk: the outline changes
    // over at the halfway mark.
    let triangle = BezPath::from_svg("M0 0 L10 0 L5 8 Z").unwrap();
    assert_eq!(morph(&smile, &triangle, 0.4), smile);
    assert_eq!(morph(&smile, &triangle, 0.6), triangle);
}

#[test]
fn a_view_box_maps_path_units_onto_the_node() {
    let mut boxed = paint("M0 0 L24 24");
    boxed.view_box = Some(morf_scene::PathViewBox {
        x: 0.0,
        y: 0.0,
        width: 24.0,
        height: 12.0,
    });
    assert_eq!(boxed.to_node(48.0, 48.0), ([2.0, 4.0], [0.0, 0.0]));
    boxed.fill_mode = ImageFillMode::PreserveAspectFit;
    // Fit: the smaller scale, centred along the other axis.
    assert_eq!(boxed.to_node(48.0, 48.0), ([2.0, 2.0], [0.0, 12.0]));
    // A stroke reaches past the box by half its width, scaled with the view
    // box, times the miter limit at a mitred corner, and one pixel more.
    boxed.stroke_color = Color::rgba8(0, 0, 0, 255);
    boxed.stroke_width = 2.0;
    assert_eq!(boxed.margin(48.0, 48.0), 1.0 + 2.0 * 4.0);
}

#[test]
fn rasterising_fills_inside_and_leaves_outside_clear() {
    let mut outlines = PathOutlines::default();
    let square = paint("M4 4 H12 V12 H4 Z");
    let drawn = rasterize(&mut outlines, &square, (16.0, 16.0), 0.0, (32, 32)).unwrap();
    let at = |x: usize, y: usize| {
        let offset = (y * 32 + x) * 4;
        drawn.rgba[offset..offset + 4].to_vec()
    };
    // Drawn at twice the logical size: logical 8 is pixel 16.
    assert_eq!(at(16, 16), vec![255, 0, 0, 255]);
    assert_eq!(at(2, 2)[3], 0);
    assert_eq!(at(28, 28)[3], 0);
    // A stroke only, trimmed to nothing, draws nothing.
    let mut stroke = paint("M0 8 H16");
    stroke.fill_color = Color::rgba8(0, 0, 0, 0);
    stroke.stroke_color = Color::rgba8(255, 255, 255, 255);
    stroke.stroke_width = 4.0;
    stroke.trim_end = 0.0;
    let empty = rasterize(&mut outlines, &stroke, (16.0, 16.0), 0.0, (16, 16)).unwrap();
    assert!(empty.rgba.chunks(4).all(|pixel| pixel[3] == 0));
}

#[test]
fn a_path_node_becomes_one_path_command_with_its_numbers() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", 100.0).unwrap();
    scene.assign(root, "height", 100.0).unwrap();
    let path = scene.create(Element::Path);
    scene.assign(path, "d", "M0 0 A10 10 0 0 1 20 20").unwrap();
    scene
        .assign(
            path,
            "view_box",
            Value::List(vec![
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(20.0),
                Value::Number(10.0),
            ]),
        )
        .unwrap();
    scene.assign(path, "trim_end", 0.25).unwrap();
    scene
        .assign(
            path,
            "dash",
            Value::List(vec![Value::Number(2.0), Value::Number(1.0)]),
        )
        .unwrap();
    scene.reparent(path, Some(root)).unwrap();
    let layout = Layout::compute(
        &scene,
        root,
        Size {
            width: 100.0,
            height: 100.0,
        },
        &mut NoText,
    )
    .unwrap();
    // The view box is its own size.
    let bounds = layout.geometry(path).unwrap();
    assert_eq!((bounds.width, bounds.height), (20.0, 10.0));
    let list = DrawList::from_scene(&scene, &layout).unwrap();
    let DrawCommand::Path { paint, .. } = &list.commands[0] else {
        panic!("a path draws as a path: {:?}", list.commands);
    };
    assert_eq!(paint.trim_end, 0.25);
    assert_eq!(paint.dash, vec![2.0, 1.0]);

    // Words and data that mean nothing are refused where they are written.
    assert!(scene.assign(path, "stroke_cap", "blunt").is_err());
    assert!(scene.assign(path, "d", "M0 0 Q").is_err());
    assert!(
        scene
            .assign(path, "view_box", Value::List(vec![Value::Number(1.0)]))
            .is_err()
    );
}

#[test]
fn a_path_is_drawn_over_its_outline_not_its_whole_node() {
    // Ruler ticks along the top edge of a fullscreen node: a strip.
    let mut ticks = paint("M10 10 V5 M90 10 V5 M170 10 V5");
    ticks.fill_color = Color::rgba8(0, 0, 0, 0);
    ticks.stroke_color = Color::rgba8(255, 255, 255, 255);
    ticks.stroke_width = 1.0;
    let mut outlines = PathOutlines::default();
    let margin = ticks.margin(3840.0, 2160.0);
    let extent = drawn_extent(&mut outlines, &ticks, (3840.0, 2160.0), margin).unwrap();
    assert!(
        extent.width < 200.0 && extent.height < 20.0,
        "drawn over {extent:?}"
    );
    assert!(
        extent.x <= 10.0 - 0.5 && extent.y <= 5.0 - 0.5,
        "the ticks are inside it"
    );
    // And a path that fills its node still covers all of it.
    let square = paint("M0 0 H16 V16 H0 Z");
    let whole = drawn_extent(&mut outlines, &square, (16.0, 16.0), 1.0).unwrap();
    assert_eq!(
        (whole.x, whole.y, whole.width, whole.height),
        (-1.0, -1.0, 18.0, 18.0)
    );
}
