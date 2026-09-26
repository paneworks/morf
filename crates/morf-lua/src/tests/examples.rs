use morf_scene::{Element, Value as SceneValue};
use std::time::Duration;

use super::*;

// The shipped examples, exercised through the runtime they are written for.

#[test]
fn fluid_transform_example_animates_square_to_circle_in_rust() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "examples/demos/motion/fluid-transform.lua",
            include_bytes!("../../../../examples/demos/motion/fluid-transform.lua"),
        )
        .unwrap();
    runtime.tick_animations(Duration::from_secs(2)).unwrap();
    let root = runtime.scene().roots()[0];
    let shape = runtime.scene().children(root).unwrap()[1];
    let pointer = runtime.scene().children(shape).unwrap()[0];
    assert_eq!(runtime.scene().number(shape, "radius").unwrap(), 12.0);

    assert_eq!(
        runtime.scene().element(pointer).unwrap(),
        Element::MouseArea
    );
    assert!(runtime.dispatch_ui_event(pointer, UiEvent::Clicked));
    assert_eq!(
        runtime.scene().target(shape, "radius").unwrap(),
        &SceneValue::Number(60.0)
    );
    assert_eq!(
        runtime.scene().target(shape, "translate_x").unwrap(),
        &SceneValue::Number(270.0)
    );

    let frame = runtime.tick_animations(Duration::from_millis(16)).unwrap();
    let radius = runtime.scene().number(shape, "radius").unwrap();
    assert!(radius > 12.0 && radius < 60.0);
    assert!(frame.active);
    assert!(frame.changed > 0, "the transform advanced");
    let moved = runtime.scene().number(shape, "translate_x").unwrap();
    assert!(
        moved != 0.0,
        "and it is the translation that moved: {moved}"
    );
}

#[test]
fn morph_stack_example_combines_native_animation_and_geometry() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "examples/demos/sdf/morph-stack.lua",
            include_bytes!("../../../../examples/demos/sdf/morph-stack.lua"),
        )
        .unwrap();
    runtime.tick_animations(Duration::from_secs(2)).unwrap();
    let root = runtime.scene().roots()[0];
    let root_children = runtime.scene().children(root).unwrap().to_vec();
    let field = root_children[10];
    let shape = runtime.scene().children(field).unwrap()[0];
    let second_stage = root_children[6];
    let second_stage_children = runtime.scene().children(second_stage).unwrap().to_vec();
    let pointer = second_stage_children[4];

    assert_eq!(runtime.scene().element(field).unwrap(), Element::Sdf);
    assert_eq!(
        runtime.scene().string_value(shape, "shape").unwrap(),
        "circle"
    );
    assert_eq!(
        runtime.scene().string_value(shape, "morph_to").unwrap(),
        "star"
    );
    assert_eq!(
        runtime.scene().element(pointer).unwrap(),
        Element::MouseArea
    );
    assert!(runtime.dispatch_ui_event(pointer, UiEvent::Clicked));
    assert_eq!(
        runtime.scene().target(shape, "morph_progress").unwrap(),
        &SceneValue::Number(1.0 / 3.0)
    );

    let frame = runtime.tick_animations(Duration::from_millis(16)).unwrap();
    let progress = runtime.scene().number(shape, "morph_progress").unwrap();
    assert!(progress > 0.0 && progress < 1.0);
    assert!(frame.changed > 0, "a property advanced");
}
#[test]
fn motion_lab_example_drives_loops_shapes_and_field_edges_in_rust() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "examples/demos/motion/motion-lab.lua",
            include_bytes!("../../../../examples/demos/motion/motion-lab.lua"),
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let children = runtime.scene().children(root).unwrap().to_vec();
    let of_element = |element| {
        children
            .iter()
            .copied()
            .find(|node| runtime.scene().element(*node).unwrap() == element)
            .expect("example is missing an element the test needs")
    };
    let badge = of_element(Element::Sdf);
    let glyph = of_element(Element::Image);

    // The badge morphs between two shape families as fields, and its point
    // count is an ordinary number that may sit between whole values.
    let layer = runtime.scene().children(badge).unwrap()[0];
    assert_eq!(runtime.scene().element(layer).unwrap(), Element::SdfShape);
    assert_eq!(
        runtime.scene().string_value(layer, "shape").unwrap(),
        "circle"
    );
    assert_eq!(
        runtime.scene().string_value(layer, "morph_to").unwrap(),
        "star"
    );
    assert_eq!(runtime.scene().number(layer, "points").unwrap(), 5.0);

    // The intro group runs first and reports its completion into Lua, which is
    // the one thing in this example that does run Lua on a tick.
    let mut completed = false;
    for _ in 0..20 {
        let frame = runtime.tick_animations(Duration::from_millis(50)).unwrap();
        completed |= !frame.groups.is_empty();
    }
    assert!(completed, "the intro group never reported its completion");
    // The group's parallel leg targets a second node, which ends up faded in.
    assert_eq!(runtime.scene().number(glyph, "opacity").unwrap(), 1.0);
    // The field edge is a plain animatable property sitting at its idle value.
    assert_eq!(runtime.scene().number(glyph, "thickness").unwrap(), 0.0);

    // With the group done, the endless behaviors keep asking for frames without
    // running a single Lua effect to do it.
    let runs = runtime.effect_runs();
    for _ in 0..40 {
        let frame = runtime.tick_animations(Duration::from_millis(50)).unwrap();
        assert!(frame.active);
        assert!(frame.groups.is_empty());
    }
    assert_eq!(runtime.effect_runs(), runs);

    // Pausing the sweep holds it in place; resuming continues from there.
    // Found by its target rather than its current value: a ping-pong sweep
    // passes through zero twice a cycle.
    let sweep = children
        .iter()
        .copied()
        .find(|node| {
            runtime.scene().element(*node).unwrap() == Element::Rect
                && runtime.scene().target(*node, "translate_x").unwrap()
                    == &SceneValue::Number(252.0)
        })
        .expect("example is missing its sweep");
    runtime
        .scene_mut()
        .set_animation_paused(sweep, "translate_x", true)
        .unwrap();
    let held = runtime.scene().number(sweep, "translate_x").unwrap();
    runtime.tick_animations(Duration::from_millis(100)).unwrap();
    assert_eq!(runtime.scene().number(sweep, "translate_x").unwrap(), held);
}

#[test]
fn clipboard_history_example_keeps_copies_and_drops() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "examples/demos/desktop/clipboard-history.lua",
            include_bytes!("../../../../examples/demos/desktop/clipboard-history.lua"),
        )
        .unwrap();
    assert!(runtime.watches_clipboard());
    let copy = |runtime: &mut Runtime, id: u64, text: &str| {
        runtime.dispatch_selection(
            false,
            Some(OfferDescription {
                id,
                mime_types: vec!["text/plain;charset=utf-8".to_owned()],
                ..OfferDescription::default()
            }),
        );
        let reads = runtime.take_offer_reads();
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0].mime, "text");
        runtime.dispatch_offer_read(reads[0].id, Ok(text.as_bytes().to_vec()));
    };
    for index in 0..10 {
        copy(&mut runtime, index, &format!("copy {index}"));
    }
    // The same text again moves nothing.
    copy(&mut runtime, 10, "copy 9");
    // The frame loop is what reconciles a Repeater with its model.
    runtime.poll_services();
    let text_of = |runtime: &Runtime| {
        let mut found = Vec::new();
        let scene = runtime.scene();
        let mut stack = scene.roots();
        while let Some(node) = stack.pop() {
            if scene.element(node).unwrap() == Element::Text {
                found.push(scene.string_value(node, "text").unwrap().to_owned());
            }
            stack.extend_from_slice(scene.children(node).unwrap());
        }
        found
    };
    let texts = text_of(&runtime);
    assert!(
        texts.iter().any(|text| text == "8 copies, newest first"),
        "{texts:?}"
    );
    assert!(texts.iter().any(|text| text == "copy 9"), "{texts:?}");
    assert!(
        !texts.iter().any(|text| text == "copy 1"),
        "only the last eight stay"
    );

    // A drop of files onto the strip joins the history as their paths.
    let mut drop_area = None;
    let scene = runtime.scene();
    let mut stack = scene.roots();
    while let Some(node) = stack.pop() {
        if scene.element(node).unwrap() == Element::DropArea {
            drop_area = Some(node);
        }
        stack.extend_from_slice(scene.children(node).unwrap());
    }
    drop(scene);
    let drop_area = drop_area.expect("the example has a drop area");
    assert_eq!(
        runtime.drop_area_keys(drop_area),
        ["image", "files", "text"]
    );
    let point = EventPoint::new((10.0, 10.0), (5.0, 5.0));
    let offer = OfferDescription {
        id: 40,
        mime_types: vec!["text/uri-list".to_owned()],
        accepted: Some("text/uri-list".to_owned()),
        uris: vec!["file:///tmp/dropped.txt".to_owned()],
        paths: vec!["/tmp/dropped.txt".to_owned()],
        ..OfferDescription::default()
    };
    runtime.dispatch_drag_entered(drop_area, point, &offer);
    assert!(
        text_of(&runtime)
            .iter()
            .any(|text| text == "drop to keep it (text/uri-list)")
    );
    runtime.dispatch_dropped(drop_area, point, &offer);
    runtime.dispatch_drag_exited(drop_area);
    runtime.poll_services();
    let texts = text_of(&runtime);
    assert!(
        texts.iter().any(|text| text == "/tmp/dropped.txt"),
        "{texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text == "drop files, text or images here")
    );
}
