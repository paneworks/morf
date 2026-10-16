//! The glass material's properties: what they accept, what the `glass` table
//! sets, and what moving them costs.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::*;

fn pane() -> (Scene, NodeHandle) {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let pane = scene.create(Element::Rect);
    scene.reparent(pane, Some(root)).unwrap();
    (scene, pane)
}

fn table(entries: &[(&str, Value)]) -> Value {
    Value::Map(
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

#[test]
fn moving_the_glass_never_moves_a_box() {
    let (mut scene, pane) = pane();
    for (property, value) in [
        ("backdrop_tint", Value::Color(Color::rgba8(10, 20, 30, 40))),
        ("backdrop_saturation", Value::Number(0.5)),
        ("backdrop_blur", Value::Number(12.0)),
        ("glass_refraction", Value::Number(8.0)),
        ("glass_frost", Value::Number(0.25)),
        ("glass_rim", Value::Number(0.5)),
        ("glass", table(&[("frost", Value::from("auto"))])),
    ] {
        let before = scene.layout_revision();
        scene.assign(pane, property, value).unwrap();
        assert_eq!(
            scene.layout_revision(),
            before,
            "`{property}` does not move a box"
        );
    }
}

#[test]
fn the_glass_table_sets_the_properties_it_names() {
    let (mut scene, pane) = pane();
    scene
        .assign(
            pane,
            "glass",
            table(&[
                ("refraction", Value::Number(12.0)),
                ("frost", Value::from("auto")),
                ("rim", Value::Number(0.4)),
                ("tint", Value::from("#00000080")),
                ("blur", Value::Number(20.0)),
                ("saturation", Value::Number(1.5)),
            ]),
        )
        .unwrap();
    assert_eq!(scene.number(pane, "glass_refraction").unwrap(), 12.0);
    assert_eq!(
        Frost::of(scene.current(pane, "glass_frost").unwrap()),
        Frost::Auto
    );
    assert_eq!(scene.number(pane, "glass_rim").unwrap(), 0.4);
    assert_eq!(
        scene.color_value(pane, "backdrop_tint").unwrap().alpha,
        128.0 / 255.0
    );
    assert_eq!(scene.number(pane, "backdrop_blur").unwrap(), 20.0);
    assert_eq!(scene.number(pane, "backdrop_saturation").unwrap(), 1.5);
    // What the table leaves out keeps its value.
    scene
        .assign(pane, "glass", table(&[("rim", Value::Number(0.0))]))
        .unwrap();
    assert_eq!(scene.number(pane, "glass_refraction").unwrap(), 12.0);
    assert_eq!(scene.number(pane, "glass_rim").unwrap(), 0.0);
}

#[test]
fn glass_refuses_what_it_does_not_take() {
    let (mut scene, pane) = pane();
    let error = scene
        .assign(pane, "glass", table(&[("shine", Value::Number(1.0))]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("`shine` is not part of glass"), "{error}");
    assert!(scene.assign(pane, "glass_frost", "busy").is_err());
    assert!(scene.assign(pane, "glass", 3.0).is_err());
    // Frost is clear to frosted, and kept within that when read.
    scene.assign(pane, "glass_frost", 4.0).unwrap();
    assert_eq!(
        Frost::of(scene.current(pane, "glass_frost").unwrap()),
        Frost::Amount(1.0)
    );
    let item = scene.create(Element::Item);
    assert!(
        scene.assign(item, "glass", table(&[])).is_err(),
        "only a Rect is glass"
    );
}

#[test]
fn a_behavior_on_a_glass_property_carries_the_tables_change() {
    let (mut scene, pane) = pane();
    scene
        .set_behavior(
            pane,
            "glass_frost",
            Some(Behavior::timed(Duration::from_millis(100), Easing::Linear)),
        )
        .unwrap();
    scene.assign(pane, "glass_frost", 0.0).unwrap();
    scene.tick_animations(Duration::from_millis(200)).unwrap();
    scene
        .assign(pane, "glass", table(&[("frost", Value::Number(1.0))]))
        .unwrap();
    scene.tick_animations(Duration::from_millis(50)).unwrap();
    let halfway = scene.number(pane, "glass_frost").unwrap();
    assert!((halfway - 0.5).abs() < 0.05, "half way through: {halfway}");
}
