use std::time::Duration;

use crate::*;

#[test]
fn reparenting_preserves_identity_and_order() {
    let mut scene = Scene::new();
    let first_parent = scene.create(Element::Item);
    let second_parent = scene.create(Element::Item);
    let child = scene.create(Element::Rect);
    scene.reparent(child, Some(first_parent)).unwrap();
    scene.reparent(child, Some(second_parent)).unwrap();

    assert!(scene.children(first_parent).unwrap().is_empty());
    assert_eq!(scene.children(second_parent).unwrap(), vec![child]);
    assert_eq!(scene.parent(child).unwrap(), Some(second_parent));
}

#[test]
fn reparenting_rejects_descendant_cycles() {
    let mut scene = Scene::new();
    let parent = scene.create(Element::Item);
    let child = scene.create(Element::Item);
    scene.reparent(child, Some(parent)).unwrap();

    assert_eq!(
        scene.reparent(parent, Some(child)),
        Err(SceneError::ParentCycle)
    );
}

#[test]
fn removed_handles_are_detectably_stale() {
    let mut scene = Scene::new();
    let parent = scene.create(Element::Item);
    let child = scene.create(Element::Text);
    scene.reparent(child, Some(parent)).unwrap();
    scene.remove(parent).unwrap();

    assert!(!scene.contains(parent));
    assert!(!scene.contains(child));
    assert_eq!(scene.parent(child), Err(SceneError::StaleNode));
}

#[test]
fn properties_coerce_colors_and_update_both_levels() {
    let mut scene = Scene::new();
    let rect = scene.create(Element::Rect);
    scene.assign(rect, "color", "#7c3aed").unwrap();

    let expected = Value::Color(Color::rgba8(0x7c, 0x3a, 0xed, 0xff));
    assert_eq!(scene.current(rect, "color").unwrap(), &expected);
    assert_eq!(scene.target(rect, "color").unwrap(), &expected);
}

#[test]
fn property_errors_name_the_element_and_property() {
    let mut scene = Scene::new();
    let text = scene.create(Element::Text);

    let unknown = scene.assign(text, "radius", 4.0).unwrap_err();
    assert_eq!(unknown.to_string(), "unknown Text property `radius`");
    let wrong = scene.assign(text, "font_size", "large").unwrap_err();
    assert!(wrong.to_string().contains("Text property `font_size`"));
}

#[test]
fn text_has_regular_font_weight_by_default() {
    let mut scene = Scene::new();
    let text = scene.create(Element::Text);

    assert_eq!(scene.number(text, "font_weight").unwrap(), 400.0);
    scene.assign(text, "font_weight", 700.0).unwrap();
    assert_eq!(scene.number(text, "font_weight").unwrap(), 700.0);
}

#[test]
fn behavior_intercepts_writes_and_keeps_target_live() {
    let mut scene = Scene::new();
    let rect = scene.create(Element::Rect);
    scene
        .set_behavior(
            rect,
            "width",
            Some(Behavior {
                duration: Duration::from_millis(200),
                easing: Easing::Linear,
                rotation_direction: RotationDirection::Numerical,
                ..Behavior::default()
            }),
        )
        .unwrap();

    scene.assign(rect, "width", 100.0).unwrap();
    assert_eq!(scene.current(rect, "width").unwrap(), &Value::Number(0.0));
    assert_eq!(scene.target(rect, "width").unwrap(), &Value::Number(100.0));
    let frame = scene.tick_animations(Duration::from_millis(100)).unwrap();

    assert_eq!(scene.current(rect, "width").unwrap(), &Value::Number(50.0));
    assert_eq!(frame.changed, 1, "one property moved");
    assert!(frame.active);
}

#[test]
fn interrupted_animation_retargets_without_a_jump() {
    let mut scene = Scene::new();
    let rect = scene.create(Element::Rect);
    let behavior = Behavior {
        duration: Duration::from_millis(200),
        easing: Easing::Linear,
        rotation_direction: RotationDirection::Numerical,
        ..Behavior::default()
    };
    scene.set_behavior(rect, "opacity", Some(behavior)).unwrap();
    scene.assign(rect, "opacity", 0.0).unwrap();
    scene.tick_animations(Duration::from_millis(50)).unwrap();
    let before = scene.number(rect, "opacity").unwrap();

    scene.assign(rect, "opacity", 0.8).unwrap();
    let retargeted = scene.number(rect, "opacity").unwrap();
    scene.tick_animations(Duration::from_millis(1)).unwrap();
    let after = scene.number(rect, "opacity").unwrap();

    assert_eq!(before, retargeted);
    assert!((after - before).abs() < 0.02);
    assert_eq!(scene.target(rect, "opacity").unwrap(), &Value::Number(0.8));
}

#[test]
fn rotation_behavior_can_take_the_shortest_path() {
    let mut scene = Scene::new();
    let rect = scene.create(Element::Rect);
    scene.assign(rect, "rotation", 350.0).unwrap();
    scene
        .set_behavior(
            rect,
            "rotation",
            Some(Behavior {
                duration: Duration::from_millis(100),
                easing: Easing::Linear,
                rotation_direction: RotationDirection::Shortest,
                ..Behavior::default()
            }),
        )
        .unwrap();

    scene.assign(rect, "rotation", 10.0).unwrap();
    scene.tick_animations(Duration::from_millis(50)).unwrap();

    assert!(scene.number(rect, "rotation").unwrap().abs() < 0.000_001);
    scene.tick_animations(Duration::from_millis(50)).unwrap();
    assert_eq!(scene.number(rect, "rotation").unwrap(), 10.0);
}

#[test]
fn paint_animation_finishes_at_the_exact_target() {
    let mut scene = Scene::new();
    let rect = scene.create(Element::Rect);
    scene
        .set_behavior(
            rect,
            "color",
            Some(Behavior {
                duration: Duration::from_millis(120),
                easing: Easing::OutCubic,
                rotation_direction: RotationDirection::Numerical,
                ..Behavior::default()
            }),
        )
        .unwrap();
    scene.assign(rect, "color", "#7c3aed").unwrap();

    let frame = scene.tick_animations(Duration::from_millis(120)).unwrap();

    assert_eq!(scene.current(rect, "color"), scene.target(rect, "color"));
    assert_eq!(frame.changed, 1, "one property moved");
    assert!(!frame.active);
}

#[test]
fn easing_families_preserve_endpoints_and_interpolate() {
    for easing in [
        Easing::Linear,
        Easing::InQuad,
        Easing::OutQuad,
        Easing::InOutQuad,
        Easing::InCubic,
        Easing::OutCubic,
        Easing::InOutCubic,
        Easing::InQuart,
        Easing::OutQuart,
        Easing::InOutQuart,
        Easing::InQuint,
        Easing::OutQuint,
        Easing::InOutQuint,
        Easing::InSine,
        Easing::OutSine,
        Easing::InOutSine,
        Easing::InExpo,
        Easing::OutExpo,
        Easing::InOutExpo,
        Easing::InCirc,
        Easing::OutCirc,
        Easing::InOutCirc,
        Easing::InBack,
        Easing::OutBack,
        Easing::InOutBack,
        Easing::InBounce,
        Easing::OutBounce,
        Easing::InOutBounce,
    ] {
        assert!((easing.value_at(0.0) - 0.0).abs() < 1e-9);
        assert!((easing.value_at(1.0) - 1.0).abs() < 1e-9);
    }
    assert_eq!(Easing::InQuad.interpolate(0.5, 10.0, 20.0), 12.5);
    assert_eq!(Easing::OutQuad.value_at(-1.0), 0.0);
    assert_eq!(Easing::OutQuad.value_at(2.0), 1.0);
}

#[test]
fn spring_retargets_with_continuous_position_and_velocity() {
    let mut scene = Scene::new();
    let item = scene.create(Element::Item);
    scene
        .set_physics(
            item,
            "x",
            Some(Physics::Spring {
                mass: 1.0,
                damping: 18.0,
                stiffness: 180.0,
                epsilon: 0.001,
            }),
        )
        .unwrap();
    scene.assign(item, "x", 100.0).unwrap();
    scene.tick_animations(Duration::from_millis(80)).unwrap();
    let before = scene.number(item, "x").unwrap();

    scene.assign(item, "x", -20.0).unwrap();
    assert_eq!(scene.number(item, "x").unwrap(), before);
    scene.tick_animations(Duration::from_millis(1)).unwrap();
    assert!((scene.number(item, "x").unwrap() - before).abs() < 2.0);
}

#[test]
fn smoothed_motion_obeys_velocity_limit() {
    let mut scene = Scene::new();
    let item = scene.create(Element::Item);
    scene
        .set_physics(item, "x", Some(Physics::Smoothed { velocity: 200.0 }))
        .unwrap();
    scene.assign(item, "x", 100.0).unwrap();

    let frame = scene.tick_animations(Duration::from_millis(100)).unwrap();
    assert_eq!(scene.number(item, "x").unwrap(), 20.0);
    assert!(frame.active);
    scene.tick_animations(Duration::from_millis(400)).unwrap();
    assert_eq!(scene.number(item, "x").unwrap(), 100.0);
}

mod exit;
mod gradient;
mod groups;
mod physics;
mod playback;
mod spline;
mod stretch;

#[test]
fn the_layout_revision_moves_only_when_geometry_does() {
    // What the revision is for: a frame that changes a colour, an opacity or a
    // rotation must be able to reuse the layout it already has, because none of
    // those move a box. Layout is the most expensive thing a frame does.
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let rect = scene.create(Element::Rect);
    scene.reparent(rect, Some(root)).unwrap();

    let settled = scene.layout_revision();
    for property in ["color", "opacity", "rotation", "scale", "border_color"] {
        let before = scene.layout_revision();
        let value: Value = match property {
            "color" | "border_color" => Value::Color(Color::rgba8(1, 2, 3, 4)),
            _ => Value::Number(0.5),
        };
        scene.assign(rect, property, value).unwrap();
        assert_eq!(
            scene.layout_revision(),
            before,
            "`{property}` does not move a box"
        );
    }
    assert_eq!(scene.layout_revision(), settled);

    // Geometry does move one, and so does the shape of the tree.
    for property in ["x", "width", "implicit_height"] {
        let before = scene.layout_revision();
        scene.assign(rect, property, 12.0).unwrap();
        assert_ne!(scene.layout_revision(), before, "`{property}` moves a box");
    }
    let before = scene.layout_revision();
    let extra = scene.create(Element::Rect);
    scene.reparent(extra, Some(root)).unwrap();
    assert_ne!(scene.layout_revision(), before, "a new child moves boxes");
    let before = scene.layout_revision();
    scene.remove(extra).unwrap();
    assert_ne!(scene.layout_revision(), before, "so does losing one");
}

#[test]
fn a_change_moves_only_its_own_trees_layout_revision() {
    // Two surfaces' trees: a bar whose clock ticks, and a settings window.
    let mut scene = Scene::new();
    let bar = scene.create(Element::Item);
    let clock = scene.create(Element::Text);
    scene.reparent(clock, Some(bar)).unwrap();
    let window = scene.create(Element::Item);
    let page = scene.create(Element::Column);
    let row = scene.create(Element::Rect);
    scene.reparent(page, Some(window)).unwrap();
    scene.reparent(row, Some(page)).unwrap();

    let bar_before = scene.layout_revision_of(bar);
    let window_before = scene.layout_revision_of(window);
    scene.assign(clock, "text", "12:01").unwrap();
    assert_ne!(scene.layout_revision_of(bar), bar_before, "the bar moved");
    assert_eq!(
        scene.layout_revision_of(window),
        window_before,
        "the window did not"
    );
    // Asked of a node inside a tree, the answer is the tree's.
    assert_eq!(scene.layout_revision_of(row), window_before);

    // An animation ticking on the bar leaves the window alone too.
    scene
        .set_behavior(
            clock,
            "x",
            Some(Behavior::timed(Duration::from_millis(100), Easing::Linear)),
        )
        .unwrap();
    scene.assign(clock, "x", 40.0).unwrap();
    scene.tick_animations(Duration::from_millis(50)).unwrap();
    assert_eq!(scene.layout_revision_of(window), window_before);

    // A node moving between the trees moves both.
    let (bar_before, window_before) = (
        scene.layout_revision_of(bar),
        scene.layout_revision_of(window),
    );
    scene.reparent(row, Some(bar)).unwrap();
    assert_ne!(scene.layout_revision_of(bar), bar_before);
    assert_ne!(scene.layout_revision_of(window), window_before);

    // And once there, it is the bar's: a change to it no longer touches the
    // window.
    let window_before = scene.layout_revision_of(window);
    scene.assign(row, "height", 30.0).unwrap();
    assert_eq!(scene.layout_revision_of(window), window_before);

    // Made, reordered or removed inside the window: the window's.
    let bar_before = scene.layout_revision_of(bar);
    let extra = scene.create(Element::Rect);
    assert_eq!(scene.layout_revision_of(bar), bar_before);
    scene.reparent(extra, Some(page)).unwrap();
    assert_ne!(scene.layout_revision_of(window), window_before);
    let window_before = scene.layout_revision_of(window);
    scene.remove(extra).unwrap();
    assert_ne!(scene.layout_revision_of(window), window_before);
    assert_eq!(scene.layout_revision_of(bar), bar_before);

    // Taken out of the window, a subtree is a tree of its own, and the
    // window still hears that it lost it.
    let window_before = scene.layout_revision_of(window);
    scene.reparent(page, None).unwrap();
    assert_ne!(scene.layout_revision_of(window), window_before);
    let page_before = scene.layout_revision_of(page);
    let window_before = scene.layout_revision_of(window);
    scene.assign(page, "width", 4.0).unwrap();
    assert_ne!(scene.layout_revision_of(page), page_before);
    assert_eq!(scene.layout_revision_of(window), window_before);
}

#[test]
fn an_animation_moves_the_layout_revision_only_on_geometry_frames() {
    // The same rule while a behavior is running: an easing colour must not
    // invalidate layout on every one of its frames.
    let mut scene = Scene::new();
    let rect = scene.create(Element::Rect);
    scene
        .set_behavior(
            rect,
            "color",
            Some(Behavior::timed(Duration::from_millis(100), Easing::Linear)),
        )
        .unwrap();
    scene
        .assign(rect, "color", Value::Color(Color::rgba8(9, 9, 9, 255)))
        .unwrap();

    let before = scene.layout_revision();
    let frame = scene.tick_animations(Duration::from_millis(50)).unwrap();
    assert!(frame.changed > 0, "the colour advanced");
    assert_eq!(
        scene.layout_revision(),
        before,
        "an easing colour never moves a box"
    );
}

#[test]
fn removing_nodes_frees_their_property_signals() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let baseline = scene.property_signal_count();
    for _ in 0..50 {
        let child = scene.create(Element::Rect);
        let grandchild = scene.create(Element::Text);
        scene.reparent(child, Some(root)).unwrap();
        scene.reparent(grandchild, Some(child)).unwrap();
        scene.remove(child).unwrap();
    }
    assert_eq!(scene.property_signal_count(), baseline);
    assert!(scene.children(root).unwrap().is_empty());
}

#[test]
fn a_display_clock_starts_an_animation_on_the_tick_after_it_was_asked_for() {
    let mut scene = Scene::new();
    scene.set_start_on_tick(true);
    let rect = scene.create(Element::Rect);
    let behavior = Behavior::timed(Duration::from_millis(200), Easing::Linear);
    scene.set_behavior(rect, "width", Some(behavior)).unwrap();
    scene.assign(rect, "width", 100.0).unwrap();

    // The frame after a stall: its delta is time the animation never had.
    let frame = scene.tick_animations(Duration::from_millis(150)).unwrap();
    assert_eq!(scene.number(rect, "width").unwrap(), 0.0);
    assert!(frame.active);
    scene.tick_animations(Duration::from_millis(100)).unwrap();
    assert_eq!(scene.number(rect, "width").unwrap(), 50.0);

    // Only its first tick is free: a later stall is time it had.
    scene.tick_animations(Duration::from_millis(50)).unwrap();
    assert_eq!(scene.number(rect, "width").unwrap(), 75.0);

    // A hand-driven clock means its deltas exactly.
    let mut manual = Scene::new();
    let other = manual.create(Element::Rect);
    manual.set_behavior(other, "width", Some(behavior)).unwrap();
    manual.assign(other, "width", 100.0).unwrap();
    manual.tick_animations(Duration::from_millis(100)).unwrap();
    assert_eq!(manual.number(other, "width").unwrap(), 50.0);
}

#[test]
fn a_restarting_behavior_retargets_like_qt_from_where_it_is_over_the_whole_curve() {
    let mut scene = Scene::new();
    let rect = scene.create(Element::Rect);
    let behavior = Behavior {
        keep_velocity: false,
        ..Behavior::timed(Duration::from_millis(400), Easing::OutCubic)
    };
    scene.set_behavior(rect, "width", Some(behavior)).unwrap();
    scene.assign(rect, "width", 400.0).unwrap();
    scene.tick_animations(Duration::from_millis(100)).unwrap();
    let from = scene.number(rect, "width").unwrap();
    assert!(
        from > 100.0,
        "out_cubic is past a quarter at a quarter: {from}"
    );

    // Sent back mid-flight: out_cubic from `from` to 0 over the full 400 ms.
    scene.assign(rect, "width", 0.0).unwrap();
    scene.tick_animations(Duration::from_millis(0)).unwrap();
    assert_eq!(scene.number(rect, "width").unwrap(), from);
    scene.tick_animations(Duration::from_millis(200)).unwrap();
    let eased = 1.0 - (1.0f64 - 0.5).powi(3);
    let expected = from * (1.0 - eased);
    let now = scene.number(rect, "width").unwrap();
    assert!((now - expected).abs() < 0.5, "{now} vs {expected}");
    // The whole duration again, not what was left of the first.
    assert!(
        scene
            .tick_animations(Duration::from_millis(150))
            .unwrap()
            .active
    );
    scene.tick_animations(Duration::from_millis(51)).unwrap();
    assert_eq!(scene.number(rect, "width").unwrap(), 0.0);
}
