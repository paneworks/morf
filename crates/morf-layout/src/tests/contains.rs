use super::*;

#[test]
fn a_node_contains_a_point_whatever_is_over_it() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let panel = scene.create(Element::Item);
    scene.assign(panel, "x", 10.0).unwrap();
    scene.assign(panel, "y", 10.0).unwrap();
    scene.assign(panel, "width", 60.0).unwrap();
    scene.assign(panel, "height", 30.0).unwrap();
    scene.assign(panel, "clip", true).unwrap();
    scene.reparent(panel, Some(root)).unwrap();
    // A button on the panel, and a row that reaches past the panel's clip.
    let button = scene.create(Element::MouseArea);
    let row = scene.create(Element::Item);
    for (node, x, width) in [(button, 5.0, 20.0), (row, 40.0, 50.0)] {
        scene.assign(node, "x", x).unwrap();
        scene.assign(node, "width", width).unwrap();
        scene.assign(node, "height", 10.0).unwrap();
        scene.reparent(node, Some(panel)).unwrap();
    }
    let layout = Layout::compute(
        &scene,
        root,
        Size {
            width: 100.0,
            height: 60.0,
        },
        &mut FixedText,
    )
    .unwrap();
    // Over the button: the button is hit, and the panel still contains it.
    assert_eq!(
        layout
            .hit_test(&scene, 20.0, 15.0)
            .unwrap()
            .map(|hit| hit.node),
        Some(button)
    );
    assert!(layout.contains_point(&scene, panel, 20.0, 15.0));
    assert!(layout.contains_point(&scene, button, 20.0, 15.0));
    assert!(layout.contains_point(&scene, root, 20.0, 15.0));
    assert!(!layout.contains_point(&scene, panel, 5.0, 15.0));
    // The row inside the clip, and past it.
    assert!(layout.contains_point(&scene, row, 60.0, 15.0));
    assert!(!layout.contains_point(&scene, row, 80.0, 15.0));
    // Moved by a transform, the panel's box moves with it.
    scene.assign(panel, "translate_x", 20.0).unwrap();
    assert!(!layout.contains_point(&scene, panel, 20.0, 15.0));
    assert!(layout.contains_point(&scene, panel, 85.0, 15.0));
    // A disabled panel is still where it is drawn.
    scene.assign(panel, "enabled", false).unwrap();
    assert!(layout.contains_point(&scene, panel, 85.0, 15.0));
    // Hidden, it contains nothing, and neither does anything on it.
    scene.assign(panel, "visible", false).unwrap();
    assert!(!layout.contains_point(&scene, panel, 85.0, 15.0));
    assert!(!layout.contains_point(&scene, button, 40.0, 15.0));
}

#[test]
fn a_node_on_another_layout_contains_nothing() {
    let mut scene = Scene::new();
    let first = scene.create(Element::Item);
    let second = scene.create(Element::Item);
    for node in [first, second] {
        scene.assign(node, "width", 50.0).unwrap();
        scene.assign(node, "height", 50.0).unwrap();
    }
    let size = Size {
        width: 50.0,
        height: 50.0,
    };
    let layout = Layout::compute(&scene, first, size, &mut FixedText).unwrap();
    assert!(layout.contains_point(&scene, first, 10.0, 10.0));
    assert!(!layout.contains_point(&scene, second, 10.0, 10.0));
}
