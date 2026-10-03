//! A mask is laid out in the box of the node it masks, whatever that node
//! is: filling it unless it says otherwise, never given a place by a
//! positioner and never scrolled.

use super::*;

fn sized(scene: &mut Scene, element: Element, width: f64, height: f64) -> NodeHandle {
    let node = scene.create(element);
    scene.assign(node, "width", width).unwrap();
    scene.assign(node, "height", height).unwrap();
    node
}

fn compute(scene: &Scene, root: NodeHandle) -> Layout {
    Layout::compute(
        scene,
        root,
        Size {
            width: 200.0,
            height: 200.0,
        },
        &mut FixedText,
    )
    .unwrap()
}

#[test]
fn a_mask_with_no_size_fills_its_owner() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let owner = sized(&mut scene, Element::Rect, 80.0, 40.0);
    scene.assign(owner, "x", 10.0).unwrap();
    scene.assign(owner, "y", 20.0).unwrap();
    scene.reparent(owner, Some(root)).unwrap();
    let mask = scene.create(Element::Rect);
    scene.set_mask(owner, Some(mask)).unwrap();
    let layout = compute(&scene, root);
    assert_eq!(
        layout.geometry(mask),
        Some(Geometry {
            x: 10.0,
            y: 20.0,
            width: 80.0,
            height: 40.0
        })
    );
    // A mask of its own size, anchored, is placed as a child would be.
    let small = sized(&mut scene, Element::Rect, 10.0, 10.0);
    scene
        .assign(
            small,
            "anchors",
            Value::Map(BTreeMap::from([
                ("right".to_owned(), Value::Bool(true)),
                ("bottom".to_owned(), Value::Bool(true)),
            ])),
        )
        .unwrap();
    scene.set_mask(owner, Some(small)).unwrap();
    let layout = compute(&scene, root);
    assert_eq!(
        layout.geometry(small),
        Some(Geometry {
            x: 80.0,
            y: 50.0,
            width: 10.0,
            height: 10.0
        })
    );
}

#[test]
fn a_positioner_gives_its_mask_no_place() {
    for element in [Element::Row, Element::Column, Element::Flex] {
        let mut scene = Scene::new();
        let root = scene.create(Element::Item);
        let row = sized(&mut scene, element, 100.0, 100.0);
        scene.reparent(row, Some(root)).unwrap();
        let first = sized(&mut scene, Element::Rect, 20.0, 20.0);
        scene.reparent(first, Some(row)).unwrap();
        let mask = scene.create(Element::Rect);
        scene
            .assign(
                mask,
                "anchors",
                Value::Map(BTreeMap::from([("fill".to_owned(), Value::Bool(true))])),
            )
            .unwrap();
        scene.set_mask(row, Some(mask)).unwrap();
        let second = sized(&mut scene, Element::Rect, 20.0, 20.0);
        scene.reparent(second, Some(row)).unwrap();
        let layout = compute(&scene, root);
        let second = layout.geometry(second).unwrap();
        assert!(
            (second.x, second.y) == (20.0, 0.0) || (second.x, second.y) == (0.0, 20.0),
            "{element:?}: the second child follows the first: {second:?}"
        );
        assert_eq!(
            layout.geometry(mask),
            Some(Geometry {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0
            }),
            "{element:?}"
        );
    }
}

#[test]
fn a_flickables_mask_stays_while_its_content_scrolls() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let list = sized(&mut scene, Element::Flickable, 100.0, 100.0);
    scene.reparent(list, Some(root)).unwrap();
    let content = sized(&mut scene, Element::Rect, 100.0, 400.0);
    scene.reparent(content, Some(list)).unwrap();
    let mask = scene.create(Element::Rect);
    scene.set_mask(list, Some(mask)).unwrap();
    scene.assign(list, "content_y", 150.0).unwrap();
    let layout = compute(&scene, root);
    assert_eq!(layout.geometry(content).unwrap().y, -150.0);
    assert_eq!(layout.geometry(mask).unwrap().y, 0.0);
    assert_eq!(layout.content_extent(&scene, list), Some((100.0, 400.0)));
}
