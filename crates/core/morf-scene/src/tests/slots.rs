use std::time::Duration;

use crate::*;

#[test]
fn removing_nodes_frees_their_property_signals() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    // The elements' shared defaults are made with their first nodes and kept.
    for element in [Element::Rect, Element::Text] {
        let first = scene.create(element);
        scene.remove(first).unwrap();
    }
    let baseline = scene.property_signal_count();
    for _ in 0..50 {
        let child = scene.create(Element::Rect);
        let grandchild = scene.create(Element::Text);
        scene.assign(child, "width", 20.0).unwrap();
        scene.assign(grandchild, "text", "hello").unwrap();
        scene.reparent(child, Some(root)).unwrap();
        scene.reparent(grandchild, Some(child)).unwrap();
        scene.remove(child).unwrap();
    }
    assert_eq!(scene.property_signal_count(), baseline);
    assert!(scene.children(root).unwrap().is_empty());
}

#[test]
fn a_node_holds_slots_only_for_what_was_set_on_it() {
    let mut scene = Scene::new();
    let first = scene.create(Element::Rect);
    let defaults = scene.property_signal_count();
    let second = scene.create(Element::Rect);
    assert_eq!(
        scene.property_signal_count(),
        defaults,
        "nothing set, nothing held"
    );
    assert_eq!(scene.number(second, "opacity").unwrap(), 1.0);

    scene.assign(first, "opacity", 1.0).unwrap();
    assert_eq!(
        scene.property_signal_count(),
        defaults,
        "a default written is not held"
    );
    scene.assign(first, "opacity", 0.25).unwrap();
    assert_eq!(scene.property_signal_count(), defaults + 2);
    assert_eq!(scene.number(first, "opacity").unwrap(), 0.25);
    assert_eq!(
        scene.number(second, "opacity").unwrap(),
        1.0,
        "the default is not shared out"
    );

    scene
        .animate_from(
            second,
            "x",
            0.0,
            100.0,
            Behavior {
                duration: Duration::from_millis(100),
                easing: Easing::Linear,
                ..Behavior::default()
            },
        )
        .unwrap();
    scene.tick_animations(Duration::from_millis(100)).unwrap();
    assert_eq!(scene.number(second, "x").unwrap(), 100.0);
    assert_eq!(scene.number(first, "x").unwrap(), 0.0);
    assert!(scene.assign(second, "no_such_property", 1.0).is_err());
}
