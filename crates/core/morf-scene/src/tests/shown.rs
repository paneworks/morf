//! An animation on a node nothing shows advances but is not motion.

use std::time::Duration;

use crate::*;

fn eased() -> Behavior {
    Behavior {
        duration: Duration::from_millis(1000),
        easing: Easing::Linear,
        rotation_direction: RotationDirection::Numerical,
        ..Behavior::default()
    }
}

#[test]
fn a_hidden_nodes_animation_is_not_motion() {
    let mut scene = Scene::new();
    let parent = scene.create(Element::Item);
    let bar = scene.create(Element::Rect);
    scene.reparent(bar, Some(parent)).unwrap();
    scene.set_behavior(bar, "width", Some(eased())).unwrap();

    // Hidden by its parent: the width still moves, and asks for nothing.
    scene.assign(parent, "visible", false).unwrap();
    scene.assign(bar, "width", 100.0).unwrap();
    let frame = scene.tick_animations(Duration::from_millis(100)).unwrap();
    assert!(
        (scene.number(bar, "width").unwrap() - 10.0).abs() < 1e-4,
        "it did not advance"
    );
    assert_eq!(frame.changed, 0, "a change nobody sees counted");
    assert!(!frame.active, "a hidden animation kept the frames coming");
    assert!(!scene.has_motion());

    // Shown again, it is motion again, and picks up where it is.
    scene.assign(parent, "visible", true).unwrap();
    let frame = scene.tick_animations(Duration::from_millis(100)).unwrap();
    assert!((scene.number(bar, "width").unwrap() - 20.0).abs() < 1e-4);
    assert_eq!(frame.changed, 1);
    assert!(frame.active);
}

#[test]
fn a_fade_in_from_nothing_is_motion() {
    let mut scene = Scene::new();
    let parent = scene.create(Element::Item);
    let card = scene.create(Element::Rect);
    scene.reparent(card, Some(parent)).unwrap();
    scene.assign(card, "opacity", 0.0).unwrap();
    scene.set_behavior(card, "opacity", Some(eased())).unwrap();
    scene.assign(card, "opacity", 1.0).unwrap();
    let frame = scene.tick_animations(Duration::from_millis(100)).unwrap();
    assert!(frame.active, "a fade in from zero opacity never started");
    assert_eq!(frame.changed, 1);
}

#[test]
fn a_hidden_nodes_new_colour_lands_at_once() {
    let mut scene = Scene::new();
    let page = scene.create(Element::Item);
    let card = scene.create(Element::Rect);
    scene.reparent(card, Some(page)).unwrap();
    scene.set_behavior(card, "color", Some(eased())).unwrap();
    let red = Color::rgba8(255, 0, 0, 255);
    let blue = Color::rgba8(0, 0, 255, 255);

    // A scheme recolouring a page nobody sees keeps no animation for it.
    scene.assign(page, "visible", false).unwrap();
    scene.assign(card, "color", Value::Color(red)).unwrap();
    assert_eq!(scene.color_value(card, "color").unwrap(), red);
    assert!(scene.running_animations().is_empty(), "a hidden colour animated");

    // On show, a new colour fades as it always did.
    scene.assign(page, "visible", true).unwrap();
    scene.assign(card, "color", Value::Color(blue)).unwrap();
    assert_eq!(scene.color_value(card, "color").unwrap(), red);
    assert_eq!(scene.running_animations().len(), 1);
}
