use std::time::Duration;

use super::*;

fn list(values: &[f64]) -> Value {
    Value::List(values.iter().copied().map(Value::Number).collect())
}

fn lay_out(scene: &Scene, root: NodeHandle) -> Layout {
    Layout::compute(
        scene,
        root,
        Size {
            width: 400.0,
            height: 300.0,
        },
        &mut FixedText,
    )
    .unwrap()
}

#[test]
fn a_transform_matrix_maps_about_the_origin_and_translates_after() {
    let mut scene = Scene::new();
    let item = scene.create(Element::Item);
    let geometry = Geometry {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 40.0,
    };
    // A horizontal shear of one about the centre (60, 40), then 5 across.
    scene
        .assign(
            item,
            "transform_matrix",
            list(&[1.0, 0.0, 1.0, 1.0, 5.0, 0.0]),
        )
        .unwrap();
    let transform = node_transform(&scene, item, geometry).unwrap();
    assert_eq!(
        transform.point(60.0, 40.0),
        (65.0, 40.0),
        "origin stays, then moves 5"
    );
    assert_eq!(
        transform.point(60.0, 50.0),
        (75.0, 50.0),
        "10 down shears 10 across"
    );

    // Inside scale: scaled twice as wide after the shear.
    scene.assign(item, "scale_x", 2.0).unwrap();
    let transform = node_transform(&scene, item, geometry).unwrap();
    assert_eq!(transform.point(60.0, 50.0), (90.0, 50.0));

    // Four numbers are the linear part alone.
    scene.assign(item, "scale_x", 1.0).unwrap();
    scene
        .assign(item, "transform_matrix", list(&[0.0, 1.0, -1.0, 0.0]))
        .unwrap();
    let transform = node_transform(&scene, item, geometry).unwrap();
    let (x, y) = transform.point(70.0, 40.0);
    assert!(
        (x - 60.0).abs() < 1e-9 && (y - 50.0).abs() < 1e-9,
        "quarter turn: {x},{y}"
    );

    assert!(
        scene
            .assign(item, "transform_matrix", list(&[1.0, 0.0, 0.0]))
            .is_err(),
        "three numbers is not a matrix"
    );
}

#[test]
fn hit_testing_goes_back_through_a_transform_matrix() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let area = scene.create(Element::MouseArea);
    scene.assign(area, "x", 100.0).unwrap();
    scene.assign(area, "y", 100.0).unwrap();
    scene.assign(area, "width", 50.0).unwrap();
    scene.assign(area, "height", 50.0).unwrap();
    // Moved 100 to the right by its matrix alone.
    scene
        .assign(
            area,
            "transform_matrix",
            list(&[1.0, 0.0, 0.0, 1.0, 100.0, 0.0]),
        )
        .unwrap();
    scene.reparent(area, Some(root)).unwrap();
    let layout = lay_out(&scene, root);
    assert!(
        layout.hit_test(&scene, 120.0, 120.0).unwrap().is_none(),
        "where it was laid"
    );
    assert_eq!(
        layout
            .hit_test(&scene, 220.0, 120.0)
            .unwrap()
            .map(|hit| hit.node),
        Some(area),
        "where it is drawn"
    );
}

/// Moves a stretching node across and reports each frame, as a paint does.
fn run(scene: &mut Scene, root: NodeHandle, frames: usize) -> Vec<Option<[f64; 4]>> {
    let mut seen = Vec::new();
    for _ in 0..frames {
        scene.tick_animations(Duration::from_millis(16)).unwrap();
        let layout = lay_out(scene, root);
        observe_stretch(scene, &layout).unwrap();
        seen.push(scene.deformation(scene.children(root).unwrap()[0]));
    }
    seen
}

#[test]
fn a_stretching_node_lengthens_along_its_motion_and_settles_when_it_stops() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let panel = scene.create(Element::Item);
    scene.assign(panel, "width", 100.0).unwrap();
    scene.assign(panel, "height", 50.0).unwrap();
    scene.reparent(panel, Some(root)).unwrap();
    scene
        .set_stretch(
            panel,
            Some(Stretch {
                stiffness: 300.0,
                damping: 14.0,
                scale: 0.2,
                max: 0.5,
            }),
        )
        .unwrap();
    scene
        .set_behavior(
            panel,
            "translate_y",
            Some(Behavior::timed(Duration::from_millis(400), Easing::Linear)),
        )
        .unwrap();
    // Nothing has moved: no deformation, no motion.
    run(&mut scene, root, 2);
    assert_eq!(scene.deformation(panel), None);
    assert!(!scene.has_motion());

    scene.assign(panel, "translate_y", 400.0).unwrap();
    let moving = run(&mut scene, root, 20);
    let deformed = moving
        .iter()
        .flatten()
        .last()
        .copied()
        .expect("it stretched");
    // Going down at 1000 px/s: taller (yy > 1), narrower (xx < 1), no shear.
    assert!(
        deformed[3] > 1.05,
        "stretched along the motion: {deformed:?}"
    );
    assert!(deformed[0] < 0.97, "squashed across it: {deformed:?}");
    assert!(deformed[1].abs() < 1e-9 && deformed[2].abs() < 1e-9);

    // The motion stops; the spring overshoots (wider than square for a
    // moment) and comes to rest exactly square, then asks for no frames.
    let settling = run(&mut scene, root, 120);
    assert!(
        settling.iter().flatten().any(|matrix| matrix[3] < 1.0),
        "the spring overshoots past square when the motion stops"
    );
    assert_eq!(scene.deformation(panel), None, "square again");
    assert!(!scene.has_motion(), "a resting stretch asks for no frames");
}

#[test]
fn a_stretch_turns_with_a_diagonal_motion_and_about_the_centre() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let panel = scene.create(Element::Item);
    scene.assign(panel, "width", 40.0).unwrap();
    scene.assign(panel, "height", 40.0).unwrap();
    scene.reparent(panel, Some(root)).unwrap();
    scene.set_stretch(panel, Some(Stretch::default())).unwrap();
    for property in ["translate_x", "translate_y"] {
        scene
            .set_behavior(
                panel,
                property,
                Some(Behavior::timed(Duration::from_millis(500), Easing::Linear)),
            )
            .unwrap();
    }
    run(&mut scene, root, 1);
    scene.assign(panel, "translate_x", 300.0).unwrap();
    scene.assign(panel, "translate_y", 300.0).unwrap();
    run(&mut scene, root, 10);
    let [a, b, c, d] = scene.deformation(panel).unwrap();
    assert!(
        b > 0.0 && (b - c).abs() < 1e-12,
        "a symmetric shear along the diagonal"
    );
    assert!((a - d).abs() < 1e-9, "equal on both axes");
    // The centre of the node is where it would be without the stretch.
    let layout = lay_out(&scene, root);
    let geometry = layout.geometry(panel).unwrap();
    let stretched = node_transform(&scene, panel, geometry).unwrap();
    let plain = node_transform_unstretched(&scene, panel, geometry).unwrap();
    assert_eq!(stretched.point(20.0, 20.0), plain.point(20.0, 20.0));
    assert_ne!(stretched.point(0.0, 0.0), plain.point(0.0, 0.0));
}

#[test]
fn a_jump_is_not_speed() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let panel = scene.create(Element::Item);
    scene.assign(panel, "width", 40.0).unwrap();
    scene.assign(panel, "height", 40.0).unwrap();
    scene.reparent(panel, Some(root)).unwrap();
    scene.set_stretch(panel, Some(Stretch::default())).unwrap();
    run(&mut scene, root, 1);
    // Half a second of nothing, then somewhere else: no stretch.
    scene.tick_animations(Duration::from_millis(500)).unwrap();
    scene.assign(panel, "x", 300.0).unwrap();
    let layout = lay_out(&scene, root);
    observe_stretch(&mut scene, &layout).unwrap();
    assert_eq!(scene.deformation(panel), None);
}
