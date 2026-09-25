//! What a mask makes of a draw list: the masked subtree in a layer, the mask
//! in a sibling layer right after it, and damage where the mask changed.

use super::*;

fn node(scene: &mut Scene, parent: NodeHandle, element: Element, place: [f64; 4]) -> NodeHandle {
    let node = scene.create(element);
    for (property, value) in ["x", "y", "width", "height"].into_iter().zip(place) {
        scene.assign(node, property, value).unwrap();
    }
    scene.reparent(node, Some(parent)).unwrap();
    node
}

fn list(scene: &Scene, root: NodeHandle) -> DrawList {
    let layout = Layout::compute(
        scene,
        root,
        Size {
            width: 100.0,
            height: 100.0,
        },
        &mut NoText,
    )
    .unwrap();
    DrawList::from_scene(scene, &layout).unwrap()
}

/// A panel with a child, masked by a circle; and a sibling drawn after it.
fn masked_panel() -> (Scene, NodeHandle, NodeHandle, NodeHandle) {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let panel = node(&mut scene, root, Element::Rect, [10.0, 10.0, 40.0, 40.0]);
    node(&mut scene, panel, Element::Rect, [4.0, 4.0, 8.0, 8.0]);
    let circle = scene.create(Element::Rect);
    scene.assign(circle, "radius", 20.0).unwrap();
    scene.set_mask(panel, Some(circle)).unwrap();
    node(&mut scene, root, Element::Rect, [60.0, 60.0, 10.0, 10.0]);
    (scene, root, panel, circle)
}

#[test]
fn a_masked_node_is_a_layer_and_its_mask_a_sibling_layer_after_it() {
    let (scene, root, panel, circle) = masked_panel();
    let list = list(&scene, root);
    // The panel, its child, the circle, the sibling: the circle drawn once,
    // into the mask, and nowhere else.
    assert_eq!(list.commands.len(), 4);
    assert_eq!(list.commands[2].node(), circle);
    assert_eq!(list.layers.len(), 2);
    let owner = &list.layers[0];
    let mask = &list.layers[1];
    assert_eq!(owner.node, panel);
    assert_eq!(owner.commands, 0..2);
    assert_eq!(
        owner.alpha_mask,
        Some(AlphaMask {
            layer: 1,
            invert: false
        })
    );
    assert_eq!(mask.mask_for, Some(0));
    assert_eq!(mask.commands, 2..3);
    assert_eq!(mask.parent, owner.parent);
    // It holds what its owner holds, not what it reaches itself.
    assert_eq!(mask.bounds, owner.bounds);
    // The mask is laid out in the panel's box.
    let DrawCommand::Quad { bounds, .. } = &list.commands[2] else {
        panic!("the circle is a quad");
    };
    assert_eq!(
        *bounds,
        Geometry {
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 40.0
        }
    );
}

#[test]
fn a_gradient_mask_is_a_quad_across_the_node() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let panel = node(&mut scene, root, Element::Item, [0.0, 20.0, 50.0, 60.0]);
    node(&mut scene, panel, Element::Rect, [0.0, 0.0, 50.0, 60.0]);
    let stops = Value::List(vec![Value::Number(0.0), Value::Number(1.0)]);
    let gradient = Value::Map([("stops".to_owned(), stops)].into_iter().collect());
    scene
        .assign(
            panel,
            "mask",
            Value::Map([("gradient".to_owned(), gradient)].into_iter().collect()),
        )
        .unwrap();
    scene.assign(panel, "mask_invert", true).unwrap();
    let list = list(&scene, root);
    assert_eq!(list.layers.len(), 2);
    assert_eq!(
        list.layers[0].alpha_mask,
        Some(AlphaMask {
            layer: 1,
            invert: true
        })
    );
    let DrawCommand::Quad {
        node,
        bounds,
        gradient: Some(gradient),
        ..
    } = &list.commands[list.layers[1].commands.start]
    else {
        panic!("the mask is a gradient quad");
    };
    assert_eq!(*node, panel);
    assert_eq!(bounds.y, 20.0);
    assert_eq!(bounds.height, 60.0);
    assert_eq!(gradient.stops[0].color.alpha, 0.0);
    assert_eq!(gradient.stops[1].color.alpha, 1.0);
}

#[test]
fn a_shadowed_masked_node_gets_an_outer_layer_for_the_mask() {
    let (mut scene, root, panel, _) = masked_panel();
    scene
        .assign(
            panel,
            "layer",
            Value::Map(
                [
                    ("enabled".to_owned(), Value::Bool(true)),
                    (
                        "shadow_color".to_owned(),
                        Value::Color(Color::rgba8(0, 0, 0, 255)),
                    ),
                    ("shadow_offset_y".to_owned(), Value::Number(6.0)),
                ]
                .into_iter()
                .collect(),
            ),
        )
        .unwrap();
    let list = list(&scene, root);
    assert_eq!(list.layers.len(), 3);
    let (outer, inner, mask) = (&list.layers[0], &list.layers[1], &list.layers[2]);
    assert!(outer.alpha_mask.is_some() && outer.shadow_color.alpha == 0.0);
    assert_eq!(inner.parent, Some(0));
    assert!(inner.alpha_mask.is_none() && inner.shadow_color.alpha > 0.0);
    assert_eq!(mask.mask_for, Some(0));
    // The outer layer reaches as far as the shadow does.
    assert_eq!(outer.bounds, inner.bounds);
    assert!(outer.bounds.height > 40.0);
}

#[test]
fn a_hidden_mask_is_no_mask() {
    let (mut scene, root, _, circle) = masked_panel();
    scene.assign(circle, "visible", false).unwrap();
    let list = list(&scene, root);
    assert!(list.layers.is_empty(), "nothing needs a layer");
    assert_eq!(list.commands.len(), 3);
}

fn step(tracker: &mut DamageTracker, mut list: DrawList) -> Vec<DamageRect> {
    let damage = tracker.diff(&list, 120);
    tracker.retain(&mut list);
    damage
}

#[test]
fn a_mask_moving_damages_where_it_was_and_is_and_nothing_else() {
    let (mut scene, root, _, circle) = masked_panel();
    let mut tracker = DamageTracker::default();
    step(&mut tracker, list(&scene, root));
    assert!(step(&mut tracker, list(&scene, root)).is_empty());
    scene.assign(circle, "width", 20.0).unwrap();
    scene.assign(circle, "height", 20.0).unwrap();
    scene.assign(circle, "x", 5.0).unwrap();
    let damage = step(&mut tracker, list(&scene, root));
    // The circle was the panel's box and is a square inside it: all of it
    // is inside the panel, none of it near the sibling at (60, 60).
    assert!(!damage.is_empty());
    for rect in &damage {
        assert!(
            rect.x >= 9 && rect.y >= 9 && rect.x + rect.width <= 51 && rect.y + rect.height <= 51,
            "{rect:?}"
        );
    }
    // Inverting damages the masked layer's whole reach.
    let owner = scene.parent(circle).unwrap().unwrap();
    scene.assign(owner, "mask_invert", true).unwrap();
    let damage = step(&mut tracker, list(&scene, root));
    assert_eq!(
        damage,
        vec![DamageRect {
            x: 10,
            y: 10,
            width: 40,
            height: 40
        }]
    );
}
