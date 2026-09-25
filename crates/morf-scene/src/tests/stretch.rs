use crate::*;

/// The same spring integrated in ten thousand tiny semi-implicit steps.
fn integrated(position: f64, rate: f64, target: f64, k: f64, c: f64, seconds: f64) -> (f64, f64) {
    let steps = 100_000;
    let dt = seconds / steps as f64;
    let (mut x, mut v) = (position, rate);
    for _ in 0..steps {
        v += (-k * (x - target) - c * v) * dt;
        x += v * dt;
    }
    (x, v)
}

#[test]
fn the_closed_form_spring_matches_integration_in_every_regime() {
    for (k, c, name) in [
        (300.0, 10.0, "under"),
        (400.0, 40.0, "critical"),
        (100.0, 60.0, "over"),
    ] {
        for (x0, v0) in [(1.0, 0.0), (0.0, 5.0), (-0.3, 2.0)] {
            for seconds in [0.004, 0.016, 0.1, 0.5] {
                let exact = spring_step(x0, v0, 0.2, k, c, seconds);
                let numeric = integrated(x0, v0, 0.2, k, c, seconds);
                assert!(
                    (exact.0 - numeric.0).abs() < 1e-3 && (exact.1 - numeric.1).abs() < 2e-2,
                    "{name}-damped from ({x0}, {v0}) over {seconds}s: {exact:?} against {numeric:?}"
                );
            }
        }
    }
}

#[test]
fn one_long_step_is_many_short_ones() {
    // Exact, so a dropped frame lands where two frames would have.
    let (k, c) = (260.0, 16.0);
    let long = spring_step(0.4, -1.0, 0.0, k, c, 0.032);
    let half = spring_step(0.4, -1.0, 0.0, k, c, 0.016);
    let short = spring_step(half.0, half.1, 0.0, k, c, 0.016);
    assert!((long.0 - short.0).abs() < 1e-12 && (long.1 - short.1).abs() < 1e-10);
}

#[test]
fn an_overdamped_spring_never_crosses_its_target() {
    let (mut x, mut v) = (1.0, 0.0);
    for _ in 0..200 {
        (x, v) = spring_step(x, v, 0.0, 100.0, 60.0, 0.016);
        assert!(x >= 0.0);
    }
    assert!(x < 1e-2, "slow root: {x}");
}

#[test]
fn the_target_stretches_along_and_squashes_across_keeping_the_area() {
    let stretch = Stretch {
        scale: 0.1,
        max: 0.5,
        ..Stretch::default()
    };
    // Straight down at 2000 px/s: twenty percent taller.
    let [xx, xy, yy] = stretch.target([0.0, 2000.0]);
    assert!((yy - 0.2).abs() < 1e-12);
    assert!(((1.0 + xx) * (1.0 + yy) - 1.0).abs() < 1e-12, "area kept");
    assert_eq!(xy, 0.0);
    // Capped.
    let [_, _, yy] = stretch.target([0.0, 1e6]);
    assert!((yy - 0.5).abs() < 1e-12);
    // At rest, nothing.
    assert_eq!(stretch.target([0.0, 0.0]), [0.0; 3]);
    // Diagonal: the determinant is still one.
    let [xx, xy, yy] = stretch.target([1000.0, 1000.0]);
    assert!(((1.0 + xx) * (1.0 + yy) - xy * xy - 1.0).abs() < 1e-12);
}

#[test]
fn stretch_settings_are_read_and_checked() {
    use std::collections::BTreeMap;
    assert_eq!(Stretch::from_value(&Value::Nil).unwrap(), None);
    assert_eq!(
        Stretch::from_value(&Value::Bool(true)).unwrap(),
        Some(Stretch::default())
    );
    let map = |pairs: &[(&str, f64)]| {
        Value::Map(
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), Value::Number(*value)))
                .collect::<BTreeMap<_, _>>(),
        )
    };
    let read = Stretch::from_value(&map(&[("stiffness", 500.0), ("scale", 0.3)]))
        .unwrap()
        .unwrap();
    assert_eq!(read.stiffness, 500.0);
    assert_eq!(read.scale, 0.3);
    assert_eq!(read.damping, Stretch::default().damping);
    assert!(Stretch::from_value(&map(&[("stiffness", 0.0)])).is_err());
    assert!(Stretch::from_value(&map(&[("bounce", 1.0)])).is_err());
}

#[test]
fn a_layer_tracks_a_live_node_and_forgets_a_removed_one() {
    let mut scene = Scene::new();
    let shape = scene.create(Element::SdfShape);
    let panel = scene.create(Element::Item);
    assert!(scene.set_track(shape, Some(shape)).is_err());
    scene.set_track(shape, Some(panel)).unwrap();
    assert_eq!(scene.track(shape), Some(panel));
    scene.remove(panel).unwrap();
    assert_eq!(scene.track(shape), None);
}

// A node standing still but drawn a hair off each frame -- float noise
// through a transform -- is at rest: its stretch settles and stops asking
// for frames. Before, any speed at all kept it "moving" for ever.
#[test]
fn a_node_at_rest_drawn_with_noise_settles() {
    let mut scene = Scene::new();
    let node = scene.create(Element::Item);
    scene
        .set_stretch(
            node,
            Some(Stretch {
                scale: 0.16,
                ..Stretch::default()
            }),
        )
        .unwrap();
    let identity = [1.0, 0.0, 0.0, 1.0];
    // A real move first, so it has a stretch to settle from.
    for frame in 0..10 {
        scene
            .tick_animations(std::time::Duration::from_millis(16))
            .unwrap();
        scene.observe_stretch(node, [100.0, 100.0 + frame as f64 * 8.0], identity);
    }
    assert!(scene.stretch_moving());
    let mut settled_at = None;
    for frame in 0..400 {
        scene
            .tick_animations(std::time::Duration::from_millis(16))
            .unwrap();
        // Sub-pixel noise, alternating: a third of a pixel a second at most.
        let noise = if frame % 2 == 0 { 3e-3 } else { -3e-3 };
        scene.observe_stretch(node, [100.0 + noise, 172.0 - noise], identity);
        if !scene.stretch_moving() {
            settled_at = Some(frame);
            break;
        }
    }
    assert!(settled_at.is_some(), "noise kept the stretch moving");
    assert_eq!(scene.deformation(node), None);
}
