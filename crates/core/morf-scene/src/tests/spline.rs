use std::time::Duration;

use crate::*;

/// A spline with one overshooting segment then a settle, as Qt writes one.
const OVERSHOOT: [f64; 12] = [
    0.2, 0.0, 0.3, 1.4, 0.6, 1.2, // up past the end by 0.6
    0.75, 1.05, 0.9, 1.0, 1.0, 1.0, // and back down onto it
];

#[test]
fn a_one_segment_spline_is_that_cubic() {
    // (0.42, 0, 0.58, 1) is `in_out` in CSS; one segment is one cubic.
    let easing = Easing::spline(&[0.42, 0.0, 0.58, 1.0, 1.0, 1.0]).unwrap();
    let cubic = Easing::CubicBezier {
        x1: 0.42,
        y1: 0.0,
        x2: 0.58,
        y2: 1.0,
    };
    for step in 0..=20 {
        let x = f64::from(step) / 20.0;
        assert!(
            (easing.value_at(x) - cubic.value_at(x)).abs() < 2e-3,
            "at {x}: {} against {}",
            easing.value_at(x),
            cubic.value_at(x)
        );
    }
    assert_eq!(easing.value_at(0.0), 0.0);
    assert_eq!(easing.value_at(1.0), 1.0);
    assert!((easing.value_at(0.5) - 0.5).abs() < 1e-9, "symmetric curve");
}

#[test]
fn a_spline_passes_through_every_segment_end_and_overshoots_between() {
    let easing = Easing::spline(&OVERSHOOT).unwrap();
    // The joint is on the curve exactly.
    assert!((easing.value_at(0.6) - 1.2).abs() < 1e-9);
    // Above one before the settle, and back on it at the end.
    let peak = (1..100)
        .map(|step| easing.value_at(f64::from(step) / 100.0))
        .fold(f64::MIN, f64::max);
    assert!(peak > 1.15, "overshoots past the end: {peak}");
    assert_eq!(easing.value_at(1.0), 1.0);
    // Monotone in time up to the joint, then descending onto one.
    assert!(easing.value_at(0.3) < easing.value_at(0.5));
    assert!(easing.value_at(0.7) > easing.value_at(0.95));
    // Out of range progress is the ends.
    assert_eq!(easing.value_at(-3.0), 0.0);
    assert_eq!(easing.value_at(7.0), 1.0);
}

#[test]
fn spline_points_are_checked() {
    assert!(Easing::spline(&[]).is_err());
    assert!(
        Easing::spline(&[0.2, 0.0, 0.3, 1.0, 1.0]).is_err(),
        "not sixes"
    );
    assert!(
        Easing::spline(&[0.2, 0.0, 0.3, 1.0, 0.9, 1.0]).is_err(),
        "ends short of (1, 1)"
    );
    assert!(
        Easing::spline(&[0.2, 0.0, 0.3, 1.0, 0.7, 0.5, 0.8, 0.5, 0.9, 0.9, 0.6, 1.0]).is_err(),
        "a segment ending before the one before it"
    );
    assert!(
        Easing::spline(&[1.2, 0.0, 0.3, 1.0, 1.0, 1.0]).is_err(),
        "a control point past its segment in x"
    );
    assert!(Easing::spline(&[0.2, f64::NAN, 0.3, 1.0, 1.0, 1.0]).is_err());
    // y is free: overshoot and undershoot are the point.
    assert!(Easing::spline(&[0.3, -0.5, 0.6, 1.8, 1.0, 1.0]).is_ok());
}

#[test]
fn naming_the_same_points_twice_is_one_curve() {
    let Easing::Spline(first) = Easing::spline(&OVERSHOOT).unwrap() else {
        unreachable!()
    };
    let Easing::Spline(second) = Easing::spline(&OVERSHOOT).unwrap() else {
        unreachable!()
    };
    assert!(std::ptr::eq(first, second));
}

#[test]
fn a_behavior_with_a_spline_overshoots_and_lands_on_its_target() {
    let mut scene = Scene::new();
    let rect = scene.create(Element::Rect);
    scene
        .set_behavior(
            rect,
            "width",
            Some(Behavior {
                duration: Duration::from_millis(1000),
                easing: Easing::spline(&OVERSHOOT).unwrap(),
                keep_velocity: false,
                ..Behavior::default()
            }),
        )
        .unwrap();
    scene.assign(rect, "width", 100.0).unwrap();
    // At 60% of the time the curve is at its joint, 1.2: twenty past.
    scene.tick_animations(Duration::from_millis(600)).unwrap();
    let width = scene.number(rect, "width").unwrap();
    assert!((width - 120.0).abs() < 0.01, "at the joint: {width}");
    scene.tick_animations(Duration::from_millis(400)).unwrap();
    assert_eq!(scene.number(rect, "width").unwrap(), 100.0);
}
