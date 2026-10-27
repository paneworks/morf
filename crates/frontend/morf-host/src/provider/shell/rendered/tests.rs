use super::*;
use morf_layout::{Size, TextMeasurer, TextOptions};
struct NoText;
impl TextMeasurer for NoText {
    fn measure(&mut self, _: NodeHandle, _: &str, _: &str, _: f64, _: TextOptions) -> Size {
        Size::default()
    }
}
#[test]
fn registered_keyboard_distinguishes_hidden_foreign_and_ambiguous_nodes() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", 100.0).unwrap();
    scene.assign(root, "height", 100.0).unwrap();
    let keyboard = scene.create(Element::Item);
    scene.reparent(keyboard, Some(root)).unwrap();
    scene.assign(keyboard, "id", "keyboard").unwrap();
    scene.assign(keyboard, "width", 100.0).unwrap();
    scene.assign(keyboard, "height", 20.0).unwrap();
    let declaration = Value::Map(
        [
            ("role".into(), Value::String("keyboard".into())),
            ("layout".into(), Value::String("full".into())),
        ]
        .into(),
    );
    scene
        .assign(keyboard, "input_context", declaration.clone())
        .unwrap();
    let snapshot = |scene: &Scene| {
        let layout = Layout::compute(
            scene,
            root,
            Size {
                width: 100.0,
                height: 100.0,
            },
            &mut NoText,
        )
        .unwrap();
        capture(scene, &layout, root, (100, 100), 120).unwrap()
    };
    assert_eq!(snapshot(&scene).keyboard["state"], "visible");
    scene.assign(keyboard, "visible", false).unwrap();
    let hidden = snapshot(&scene).keyboard;
    assert_eq!(hidden["state"], "hidden");
    assert_eq!(hidden["registered"], 1);
    scene.reparent(keyboard, None).unwrap();
    assert_eq!(snapshot(&scene).keyboard["state"], "unknown");
    scene.reparent(keyboard, Some(root)).unwrap();
    let other = scene.create(Element::Item);
    scene.assign(other, "input_context", declaration).unwrap();
    let ambiguous = snapshot(&scene).keyboard;
    assert_eq!(ambiguous["state"], "unknown");
    assert_eq!(ambiguous["registered"], 2);
}
#[test]
fn escaped_semantics_obey_the_total_reply_budget_without_truncation() {
    let regions = (1..=LIMIT)
        .map(|node| Region {
            node: node as u64,
            id: "\"".repeat(128),
            interactive: false,
            bounds: Geometry {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
            semantic: Some(json!({"role":"keyboard","layout":"\"".repeat(128)})),
        })
        .collect();
    let mut state = Rendered {
        generation: 2,
        revision: 1,
        sampled: 1_000,
        frame: Some(Frame {
            size: (100, 100),
            space: None,
            keyboard: json!({"scope":"registered_morf_primary","registered":0,"state":"unknown"}),
            scale_120: 120,
            regions,
        }),
        ..Default::default()
    };
    let report = state.report(&"a".repeat(32), 2, 1_001);
    assert!(report["frame"].is_null());
    assert!(serde_json::to_vec(&report).unwrap().len() < 128 * 1024);
    state.frame.as_mut().unwrap().regions.truncate(1);
    let report = state.report(&"a".repeat(32), 2, 1_001);
    assert_eq!(report["frame"]["regions"].as_array().unwrap().len(), 1);
    assert_eq!(report["frame"]["regions"][0]["id"], "\"".repeat(128));
}
#[test]
fn semantic_annotations_are_typed_and_cannot_supply_geometry_or_authentication() {
    let value = |json: Json| -> Value {
        let mut fields = std::collections::BTreeMap::new();
        for (key, value) in json.as_object().unwrap() {
            fields.insert(
                key.clone(),
                match value {
                    Json::String(value) => Value::String(value.clone()),
                    _ => Value::Bool(true),
                },
            );
        }
        Value::Map(fields)
    };
    assert!(semantic(&Value::Nil).unwrap().is_none());
    assert_eq!(
        semantic(&value(json!({"role":"keyboard","layout":"dev"}))).unwrap(),
        Some(json!({"role":"keyboard","layout":"dev"}))
    );
    for invalid in [
        json!({"role":"keyboard"}),
        json!({"role":"keyboard","layout":""}),
        json!({"role":"top_left","layout":"dev"}),
        json!({"role":"unlocked"}),
        json!({"role":"keyboard","layout":"full","x":0}),
        json!({"role":"keyboard","layout":"full","authenticated":true}),
    ] {
        assert!(semantic(&value(invalid)).is_err());
    }
}
#[test]
fn named_bounds_follow_transforms_clips_visibility_and_keep_repeated_names_distinct() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", 100.0).unwrap();
    scene.assign(root, "height", 100.0).unwrap();
    let parent = scene.create(Element::Item);
    scene.reparent(parent, Some(root)).unwrap();
    for (key, value) in [
        ("x", 10.0),
        ("y", 20.0),
        ("width", 50.0),
        ("height", 50.0),
        ("translate_x", 5.0),
    ] {
        scene.assign(parent, key, value).unwrap();
    }
    scene.assign(parent, "clip", true).unwrap();
    let child = scene.create(Element::MouseArea);
    scene.reparent(child, Some(parent)).unwrap();
    scene.assign(child, "id", "key").unwrap();
    scene.assign(child, "x", 40.0).unwrap();
    scene.assign(child, "y", 5.0).unwrap();
    scene.assign(child, "width", 30.0).unwrap();
    scene.assign(child, "height", 20.0).unwrap();
    scene
        .assign(
            child,
            "input_context",
            Value::Map(
                [
                    ("role".into(), Value::String("keyboard".into())),
                    ("layout".into(), Value::String("dev".into())),
                ]
                .into(),
            ),
        )
        .unwrap();
    let compute = |scene: &Scene| {
        Layout::compute(
            scene,
            root,
            Size {
                width: 100.0,
                height: 100.0,
            },
            &mut NoText,
        )
        .unwrap()
    };
    let frame = capture(&scene, &compute(&scene), root, (100, 100), 120).unwrap();
    assert_eq!(frame.regions.len(), 1);
    assert_eq!(
        frame.regions[0].bounds,
        Geometry {
            x: 55.0,
            y: 25.0,
            width: 10.0,
            height: 20.0
        }
    );
    assert!(frame.regions[0].interactive);
    assert_eq!(
        frame.regions[0].semantic,
        Some(json!({"role":"keyboard","layout":"dev"}))
    );
    scene.assign(parent, "enabled", false).unwrap();
    assert!(
        !capture(&scene, &compute(&scene), root, (100, 100), 120)
            .unwrap()
            .regions[0]
            .interactive
    );
    scene.assign(parent, "opacity", 0.0).unwrap();
    assert!(
        capture(&scene, &compute(&scene), root, (100, 100), 120)
            .unwrap()
            .regions
            .is_empty()
    );
    scene.assign(parent, "opacity", 1.0).unwrap();
    scene.assign(parent, "id", "key").unwrap();
    let repeated = capture(&scene, &compute(&scene), root, (100, 100), 120).unwrap();
    assert_eq!(repeated.regions.len(), 2);
    assert_eq!(repeated.regions[0].id, repeated.regions[1].id);
    assert_ne!(repeated.regions[0].node, repeated.regions[1].node);
    scene.assign(parent, "visible", false).unwrap();
    assert!(
        capture(&scene, &compute(&scene), root, (100, 100), 120)
            .unwrap()
            .regions
            .is_empty()
    );
}
#[test]
fn report_never_refreshes_itself_or_crosses_a_runtime_generation() {
    let mut state = Rendered {
        revision: 7,
        generation: 2,
        sampled: 1_000,
        frame: Some(Frame {
            size: (100, 200),
            space: None,
            keyboard: json!({"scope":"registered_morf_primary","registered":0,"state":"unknown"}),
            scale_120: 120,
            regions: Vec::new(),
        }),
        ..Default::default()
    };
    assert!(state.report("epoch", 2, 1_001)["frame"].is_object());
    for (generation, now) in [(3, 1_001), (2, 999), (2, 1_000 + FRESH_NS)] {
        assert!(state.report("epoch", generation, now)["frame"].is_null());
    }
    state.busy();
    assert!(state.report("epoch", 2, 1_001)["frame"].is_null());
}
