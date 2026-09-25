//! A node on its way out keeps its box and gives up its room.

use std::time::Duration;

use morf_scene::{Behavior, ExitSpec, Value};

use super::*;

const AREA: Size = Size {
    width: 300.0,
    height: 300.0,
};

fn rect(scene: &mut Scene, parent: NodeHandle, width: f64, height: f64) -> NodeHandle {
    let node = scene.create(Element::Rect);
    scene.assign(node, "width", width).unwrap();
    scene.assign(node, "height", height).unwrap();
    scene.reparent(node, Some(parent)).unwrap();
    node
}

fn fade() -> ExitSpec {
    ExitSpec {
        values: vec![("opacity".to_owned(), Value::Number(0.0))],
        behavior: Behavior {
            duration: Duration::from_millis(200),
            ..Behavior::default()
        },
    }
}

/// A column of three with a gap, inside an item that offsets it, and a
/// mouse area filling the middle one.
fn column(scene: &mut Scene) -> (NodeHandle, NodeHandle, [NodeHandle; 3], NodeHandle) {
    let root = scene.create(Element::Item);
    let column = scene.create(Element::Column);
    scene.reparent(column, Some(root)).unwrap();
    scene.assign(column, "x", 10.0).unwrap();
    scene.assign(column, "y", 20.0).unwrap();
    scene.assign(column, "gap", 5.0).unwrap();
    let a = rect(scene, column, 40.0, 30.0);
    let b = rect(scene, column, 60.0, 40.0);
    let c = rect(scene, column, 40.0, 30.0);
    let area = scene.create(Element::MouseArea);
    scene
        .assign(
            area,
            "anchors",
            Value::Map([("fill".to_owned(), Value::Bool(true))].into()),
        )
        .unwrap();
    scene.reparent(area, Some(b)).unwrap();
    scene.set_exit(b, Some(fade())).unwrap();
    (root, column, [a, b, c], area)
}

#[test]
fn the_siblings_close_up_while_the_leaving_node_stays_where_it_was() {
    let mut scene = Scene::new();
    let (root, column, [a, b, c], area) = column(&mut scene);
    let mut layout = Layout::compute(&scene, root, AREA, &mut FixedText).unwrap();
    let was = layout.geometry(b).unwrap();
    assert_eq!(
        layout.geometry(c).unwrap().y,
        20.0 + 30.0 + 5.0 + 40.0 + 5.0
    );
    assert!(
        layout
            .hit_test(&scene, 30.0, was.y + 10.0)
            .unwrap()
            .is_some()
    );

    assert!(scene.begin_exit(b).unwrap());
    layout.update(&scene, root, AREA, &mut FixedText).unwrap();
    assert_eq!(layout.geometry(b), Some(was), "it keeps its box");
    assert_eq!(
        layout.geometry(area),
        Some(was),
        "and what is inside it keeps its place in it"
    );
    assert_eq!(
        layout.geometry(c).unwrap().y,
        20.0 + 30.0 + 5.0,
        "c closes up"
    );
    assert_eq!(layout.geometry(a).unwrap().y, 20.0);
    assert_eq!(
        layout.implicit_size(column),
        Some(Size {
            width: 40.0,
            height: 65.0
        }),
        "the column is sized without it"
    );
    assert!(
        layout
            .hit_test(&scene, 30.0, was.y + 10.0)
            .unwrap()
            .is_none(),
        "and takes no input"
    );

    // A whole pass puts it in the same place: the box is the scene's.
    let whole = Layout::compute(&scene, root, AREA, &mut FixedText).unwrap();
    assert_eq!(layout.difference(&whole), None);

    // Its parent moves, and it moves with it.
    scene.assign(column, "x", 50.0).unwrap();
    layout.update(&scene, root, AREA, &mut FixedText).unwrap();
    assert_eq!(layout.geometry(b).unwrap().x, was.x + 40.0);

    // Taken back: into the flow again, and the pointer finds it.
    scene.cancel_exit(b).unwrap();
    layout.update(&scene, root, AREA, &mut FixedText).unwrap();
    assert_eq!(
        layout.geometry(c).unwrap().y,
        20.0 + 30.0 + 5.0 + 40.0 + 5.0
    );
    assert!(
        layout
            .hit_test(&scene, 70.0, was.y + 10.0)
            .unwrap()
            .is_some()
    );
}

#[test]
fn a_leaving_node_slides_by_what_its_exit_does_to_x_and_y() {
    let mut scene = Scene::new();
    let (root, _, [_, b, _], _) = column(&mut scene);
    let mut exit = fade();
    exit.values.push(("y".to_owned(), Value::Number(12.0)));
    scene.set_exit(b, Some(exit)).unwrap();
    let mut layout = Layout::compute(&scene, root, AREA, &mut FixedText).unwrap();
    let was = layout.geometry(b).unwrap();
    scene.begin_exit(b).unwrap();
    scene.tick_animations(Duration::from_millis(100)).unwrap();
    layout.update(&scene, root, AREA, &mut FixedText).unwrap();
    assert!((layout.geometry(b).unwrap().y - (was.y + 6.0)).abs() < 1e-9);
}

#[test]
fn a_flex_parent_leaves_a_leaving_child_where_it_was_too() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let flex = scene.create(Element::Flex);
    scene.reparent(flex, Some(root)).unwrap();
    scene.assign(flex, "width", 200.0).unwrap();
    scene.assign(flex, "height", 50.0).unwrap();
    let a = rect(&mut scene, flex, 30.0, 20.0);
    let b = rect(&mut scene, flex, 30.0, 20.0);
    let c = rect(&mut scene, flex, 30.0, 20.0);
    scene.set_exit(b, Some(fade())).unwrap();
    let mut layout = Layout::compute(&scene, root, AREA, &mut FixedText).unwrap();
    let was = layout.geometry(b).unwrap();
    let c_was = layout.geometry(c).unwrap();
    scene.begin_exit(b).unwrap();
    layout.update(&scene, root, AREA, &mut FixedText).unwrap();
    assert_eq!(layout.geometry(b), Some(was));
    assert!(
        layout.geometry(c).unwrap().x < c_was.x,
        "c moves into its room"
    );
    assert_eq!(layout.geometry(a).unwrap().x, 0.0);
}

#[test]
fn a_node_that_leaves_before_it_was_ever_placed_stays_at_its_own_x_and_y() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let node = rect(&mut scene, root, 20.0, 10.0);
    scene.assign(node, "x", 7.0).unwrap();
    scene.set_exit(node, Some(fade())).unwrap();
    scene.begin_exit(node).unwrap();
    let layout = Layout::compute(&scene, root, AREA, &mut FixedText).unwrap();
    let geometry = layout.geometry(node).unwrap();
    assert_eq!((geometry.x, geometry.width), (7.0, 20.0));
}

#[test]
fn a_fresh_layout_puts_a_leaving_node_where_the_last_one_placed_it() {
    // A runner that lays out from nothing each frame, rather than bringing
    // one layout up to date, still sees the node where it was.
    let mut scene = Scene::new();
    let (root, _, [_, b, c], _) = column(&mut scene);
    let was = Layout::compute(&scene, root, AREA, &mut FixedText)
        .unwrap()
        .geometry(b)
        .unwrap();
    scene.begin_exit(b).unwrap();
    let fresh = Layout::compute(&scene, root, AREA, &mut FixedText).unwrap();
    assert_eq!(fresh.geometry(b), Some(was));
    assert_eq!(fresh.geometry(c).unwrap().y, 20.0 + 30.0 + 5.0);
}
