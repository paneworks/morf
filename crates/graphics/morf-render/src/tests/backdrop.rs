//! Frosted glass in the draw list and in the damage: where the backdrop goes,
//! what asks for it, and what has to be repainted when something beneath it
//! changes.

use super::*;

/// A 100×60 surface: a ground rect, a glass at (20, 10) sized 40×40 with
/// `backdrop_blur` set to `blur`, and a dot on top of it.
fn glass(blur: Value) -> (Scene, NodeHandle, [NodeHandle; 3]) {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let ground = scene.create(Element::Rect);
    let glass = scene.create(Element::Rect);
    let dot = scene.create(Element::Rect);
    scene.assign(root, "width", 100.0).unwrap();
    scene.assign(root, "height", 60.0).unwrap();
    for (node, x, y, width, height) in [
        (ground, 0.0, 0.0, 100.0, 60.0),
        (glass, 20.0, 10.0, 40.0, 40.0),
        (dot, 30.0, 20.0, 4.0, 4.0),
    ] {
        scene.assign(node, "x", x).unwrap();
        scene.assign(node, "y", y).unwrap();
        scene.assign(node, "width", width).unwrap();
        scene.assign(node, "height", height).unwrap();
        scene.reparent(node, Some(root)).unwrap();
    }
    scene.assign(glass, "radius", 12.0).unwrap();
    scene.assign(glass, "color", "#ffffff33").unwrap();
    scene.assign(glass, "backdrop_blur", blur).unwrap();
    (scene, root, [ground, glass, dot])
}

fn draw(scene: &Scene, root: NodeHandle) -> DrawList {
    let layout = Layout::compute(
        scene,
        root,
        Size {
            width: 100.0,
            height: 60.0,
        },
        &mut NoText,
    )
    .unwrap();
    DrawList::from_scene(scene, &layout).unwrap()
}

fn diff_frame(tracker: &mut DamageTracker, mut list: DrawList) -> Vec<DamageRect> {
    let damage = tracker.diff(&list, 120);
    tracker.retain(&mut list);
    damage
}

fn covers(damage: &[DamageRect], x: u32, y: u32, width: u32, height: u32) -> bool {
    damage.iter().any(|rect| {
        rect.x <= x
            && rect.y <= y
            && rect.x + rect.width >= x + width
            && rect.y + rect.height >= y + height
    })
}

#[test]
fn a_radius_puts_the_backdrop_just_under_the_fill() {
    let (scene, root, [_, glass, _]) = glass(Value::Number(16.0));
    let list = draw(&scene, root);
    let backdrop = list
        .commands
        .iter()
        .position(|command| matches!(command, DrawCommand::Backdrop { .. }))
        .expect("a number asks the engine to blur");
    let DrawCommand::Backdrop {
        node,
        radius,
        radii,
        saturation,
        ..
    } = &list.commands[backdrop]
    else {
        unreachable!()
    };
    assert_eq!((*node, *radius, *saturation), (glass, 16.0, 1.0));
    assert_eq!(*radii, [12.0; 4]);
    assert!(
        matches!(&list.commands[backdrop + 1], DrawCommand::Quad { node, .. } if *node == glass),
        "the glass's own fill tints the backdrop"
    );
    // The blur reaches beyond the shape, so the edge pulls in what is next
    // to it.
    let reach = list.commands[backdrop].backdrop_reach().unwrap();
    assert_eq!((reach.x, reach.y, reach.width), (-12.0, -22.0, 104.0));
}

#[test]
fn true_is_still_the_compositors_and_the_radius_is_capped() {
    let (scene, root, _) = glass(Value::Bool(true));
    assert!(
        !draw(&scene, root)
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Backdrop { .. })),
        "true asks the compositor, not the engine"
    );
    let (scene, root, _) = glass(Value::Number(1e6));
    let list = draw(&scene, root);
    assert!(list.commands.iter().any(|command| matches!(
        command,
        DrawCommand::Backdrop { radius, .. } if *radius == MAX_BACKDROP_BLUR
    )));
    let (scene, root, _) = glass(Value::Number(0.0));
    assert!(
        !draw(&scene, root)
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Backdrop { .. })),
        "zero is off"
    );
}

#[test]
fn a_change_beneath_the_glass_repaints_all_of_it() {
    let (mut scene, root, [ground, _, _]) = glass(Value::Number(4.0));
    let mut tracker = DamageTracker::default();
    diff_frame(&mut tracker, draw(&scene, root));
    // The ground changes colour only in a corner of the glass's reach; the
    // blur carries it across the whole panel.
    scene.assign(ground, "width", 22.0).unwrap();
    let damage = diff_frame(&mut tracker, draw(&scene, root));
    assert!(
        covers(&damage, 20, 10, 40, 40),
        "the whole glass is repainted: {damage:?}"
    );
}

#[test]
fn a_change_on_top_of_the_glass_repaints_only_itself() {
    let (mut scene, root, [_, _, dot]) = glass(Value::Number(4.0));
    let mut tracker = DamageTracker::default();
    diff_frame(&mut tracker, draw(&scene, root));
    scene.assign(dot, "x", 40.0).unwrap();
    let damage = diff_frame(&mut tracker, draw(&scene, root));
    assert!(!damage.is_empty());
    assert!(
        !covers(&damage, 20, 10, 40, 40),
        "the dot over the glass does not repaint all of it: {damage:?}"
    );
}
