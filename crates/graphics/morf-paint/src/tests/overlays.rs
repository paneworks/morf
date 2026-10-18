//! Trees painted over a surface's scene: after it, or in place of it.

use super::*;
use crate::paint::SpanBase;

/// A square `size` across on its own, laid out at that size.
fn square(scene: &mut Scene, size: f64) -> (NodeHandle, Layout) {
    let node = scene.create(Element::Rect);
    scene.assign(node, "width", size).unwrap();
    scene.assign(node, "height", size).unwrap();
    scene.assign(node, "color", "#ff0000").unwrap();
    let available = Size {
        width: size,
        height: size,
    };
    let layout = Layout::compute(scene, node, available, &mut NoText).unwrap();
    (node, layout)
}

#[test]
fn an_overlay_is_painted_over_the_scene_or_instead_of_it() {
    let mut scene = Scene::new();
    let (root, layout) = square(&mut scene, 20.0);
    let (cover, cover_layout) = square(&mut scene, 5.0);
    let painted = |conceals| {
        let overlay = Overlay {
            root: cover,
            layout: &cover_layout,
            conceals,
        };
        let mut list = DrawList::default();
        list.rebuild_with(&scene, &layout, &[overlay], SpanBase::default(), None)
            .unwrap();
        list.commands
            .iter()
            .map(DrawCommand::node)
            .collect::<Vec<_>>()
    };

    assert_eq!(painted(false), [root, cover]);
    assert_eq!(painted(true), [cover]);
}
