//! A mask node: a child of the node it masks that paint and hit testing
//! never see, and a `mask` property holding a gradient.

use std::collections::BTreeMap;

use crate::*;

#[test]
fn a_mask_becomes_a_child_that_paint_order_leaves_out() {
    let mut scene = Scene::new();
    let owner = scene.create(Element::Rect);
    let child = scene.create(Element::Rect);
    scene.reparent(child, Some(owner)).unwrap();
    let mask = scene.create(Element::Rect);
    scene.set_mask(owner, Some(mask)).unwrap();
    assert_eq!(scene.parent(mask).unwrap(), Some(owner));
    assert_eq!(scene.children(owner).unwrap(), vec![child, mask]);
    assert_eq!(scene.paint_order(owner).unwrap().as_ref(), &[child]);
    assert_eq!(scene.mask(owner), Some(mask));
    assert!(scene.is_mask(mask));
    assert!(!scene.is_mask(child));
}

#[test]
fn replacing_a_mask_removes_the_old_one_and_removing_the_owner_its_mask() {
    let mut scene = Scene::new();
    let owner = scene.create(Element::Item);
    let first = scene.create(Element::Rect);
    let second = scene.create(Element::Rect);
    scene.set_mask(owner, Some(first)).unwrap();
    scene.set_mask(owner, Some(second)).unwrap();
    assert!(scene.element(first).is_err(), "the old mask is gone");
    assert_eq!(scene.mask(owner), Some(second));
    scene.set_mask(owner, None).unwrap();
    assert!(scene.element(second).is_err());
    assert_eq!(scene.mask(owner), None);

    let third = scene.create(Element::Rect);
    scene.set_mask(owner, Some(third)).unwrap();
    scene.remove(owner).unwrap();
    assert!(scene.element(third).is_err());
    assert!(scene.masks.is_empty() && scene.mask_owners.is_empty());
}

#[test]
fn a_mask_moved_elsewhere_stops_masking() {
    let mut scene = Scene::new();
    let owner = scene.create(Element::Item);
    let other = scene.create(Element::Item);
    let mask = scene.create(Element::Rect);
    scene.set_mask(owner, Some(mask)).unwrap();
    scene.reparent(mask, Some(other)).unwrap();
    assert_eq!(scene.mask(owner), None);
    assert!(!scene.is_mask(mask));
    assert_eq!(scene.paint_order(other).unwrap().as_ref(), &[mask]);
}

#[test]
fn a_node_cannot_mask_itself_or_its_ancestor() {
    let mut scene = Scene::new();
    let parent = scene.create(Element::Item);
    let owner = scene.create(Element::Item);
    scene.reparent(owner, Some(parent)).unwrap();
    assert!(scene.set_mask(owner, Some(owner)).is_err());
    assert!(scene.set_mask(owner, Some(parent)).is_err());
}

#[test]
fn a_mask_gradient_takes_bare_numbers_as_alpha() {
    let mut scene = Scene::new();
    let node = scene.create(Element::Item);
    let gradient = Value::Map(BTreeMap::from([(
        "stops".to_owned(),
        Value::List(vec![
            Value::Number(0.0),
            Value::List(vec![Value::Number(1.0), Value::Number(0.2)]),
            Value::Map(BTreeMap::from([
                ("color".to_owned(), Value::Number(0.5)),
                ("position".to_owned(), Value::Number(0.9)),
            ])),
            Value::String("#ff000000".to_owned()),
        ]),
    )]));
    scene
        .assign(
            node,
            "mask",
            Value::Map(BTreeMap::from([("gradient".to_owned(), gradient)])),
        )
        .unwrap();
    let spec = MaskSpec::parse(scene.current(node, "mask").unwrap())
        .unwrap()
        .expect("a mask");
    let alphas: Vec<f32> = spec
        .gradient
        .stops
        .iter()
        .map(|stop| stop.color.alpha)
        .collect();
    assert_eq!(alphas, vec![0.0, 1.0, 0.5, 0.0]);
    let positions: Vec<f64> = spec
        .gradient
        .stops
        .iter()
        .map(|stop| stop.position)
        .collect();
    assert_eq!(positions, vec![0.0, 0.2, 0.9, 1.0]);
    // A canonical value reads back as itself.
    assert_eq!(
        MaskSpec::parse(scene.current(node, "mask").unwrap()).unwrap(),
        Some(spec)
    );
    // Nothing, or something else.
    scene.assign(node, "mask", Value::Nil).unwrap();
    assert_eq!(
        MaskSpec::parse(scene.current(node, "mask").unwrap()).unwrap(),
        None
    );
    assert!(
        scene
            .assign(
                node,
                "mask",
                Value::Map(BTreeMap::from([("radius".to_owned(), Value::Number(2.0))]))
            )
            .is_err()
    );
}
