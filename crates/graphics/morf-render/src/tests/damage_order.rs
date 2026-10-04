//! Damage when commands are added before others: the ones pushed along keep
//! their order among themselves and are not repainted for it.

use super::*;

fn quad(node: NodeHandle, x: f64, y: f64, width: f64, height: f64) -> DrawCommand {
    DrawCommand::Quad {
        node,
        bounds: Geometry {
            x,
            y,
            width,
            height,
        },
        transform: Transform2D::IDENTITY,
        clip: None,
        color: Color::rgba8(255, 255, 255, 255),
        color_overlay: Color::rgba8(0, 0, 0, 0),
        gradient: None,
        radii: [0.0; 4],
        border_width: 0.0,
        antialiasing: true,
        border_pixel_aligned: false,
        border_color: Color::rgba8(0, 0, 0, 0),
        blur: 0.0,
        shadow_color: Color::rgba8(0, 0, 0, 0),
        shadow_blur: 0.0,
        shadow_spread: 0.0,
        shadow_offset_x: 0.0,
        shadow_offset_y: 0.0,
        shadow_inner: false,
        shader: None,
    }
}

fn area(tracker: &mut DamageTracker, commands: Vec<DrawCommand>) -> u64 {
    let mut list = DrawList {
        commands,
        layers: Vec::new(),
    };
    let damage = tracker.diff(&list, 120);
    tracker.retain(&mut list);
    damage
        .iter()
        .map(|rect| u64::from(rect.width) * u64::from(rect.height))
        .sum()
}

fn nodes(count: usize) -> Vec<NodeHandle> {
    let mut scene = Scene::new();
    (0..count).map(|_| scene.create(Element::Rect)).collect()
}

#[test]
fn a_command_added_before_others_damages_only_itself() {
    let [frame, panel, swell] = nodes(3)[..] else {
        unreachable!()
    };
    let big = |node| quad(node, 0.0, 0.0, 2000.0, 1000.0);
    let mut tracker = DamageTracker::default();
    area(&mut tracker, vec![big(frame), big(panel)]);
    let added = area(
        &mut tracker,
        vec![quad(swell, 10.0, 10.0, 20.0, 20.0), big(frame), big(panel)],
    );
    assert_eq!(added, 400, "only the new command's own pixels");
    let removed = area(&mut tracker, vec![big(frame), big(panel)]);
    assert_eq!(removed, 400, "only where it was");
}

#[test]
fn commands_that_change_places_are_damaged() {
    let [a, b] = nodes(2)[..] else { unreachable!() };
    let mut tracker = DamageTracker::default();
    area(
        &mut tracker,
        vec![
            quad(a, 0.0, 0.0, 10.0, 10.0),
            quad(b, 100.0, 0.0, 10.0, 10.0),
        ],
    );
    let swapped = area(
        &mut tracker,
        vec![
            quad(b, 100.0, 0.0, 10.0, 10.0),
            quad(a, 0.0, 0.0, 10.0, 10.0),
        ],
    );
    assert!(swapped >= 100, "one of the two that swapped is repainted");
}
