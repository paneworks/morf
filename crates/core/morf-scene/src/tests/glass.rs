//! The glass material's properties: what they accept and what moving them
//! costs.

use crate::*;

#[test]
fn moving_the_glass_never_moves_a_box() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let pane = scene.create(Element::Rect);
    scene.reparent(pane, Some(root)).unwrap();
    for (property, value) in [
        ("backdrop_tint", Value::Color(Color::rgba8(10, 20, 30, 40))),
        ("backdrop_saturation", Value::Number(0.5)),
        ("backdrop_blur", Value::Number(12.0)),
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
