use std::time::Duration;

use crate::*;

fn fade(ms: u64) -> ExitSpec {
    ExitSpec {
        values: vec![
            ("opacity".to_owned(), Value::Number(0.0)),
            ("scale".to_owned(), Value::Number(0.5)),
        ],
        behavior: Behavior {
            duration: Duration::from_millis(ms),
            ..Behavior::default()
        },
    }
}

#[test]
fn an_exit_names_properties_the_node_has_and_values_it_takes() {
    let mut scene = Scene::new();
    let node = scene.create(Element::Rect);
    let mut bad = fade(100);
    bad.values.push(("opacityy".to_owned(), Value::Number(0.0)));
    assert!(matches!(
        scene.set_exit(node, Some(bad)),
        Err(SceneError::UnknownProperty { .. })
    ));
    let mut wrong = fade(100);
    wrong.values[0].1 = Value::String("none".into());
    assert!(scene.set_exit(node, Some(wrong)).is_err());
    scene.set_exit(node, Some(fade(100))).unwrap();
    assert_eq!(scene.exit_spec(node), Some(&fade(100)));
}

#[test]
fn a_node_without_an_exit_or_with_an_instant_one_is_not_kept() {
    let mut scene = Scene::new();
    let plain = scene.create(Element::Rect);
    let instant = scene.create(Element::Rect);
    scene.set_exit(instant, Some(fade(0))).unwrap();
    assert!(!scene.begin_exit(plain).unwrap());
    assert!(!scene.begin_exit(instant).unwrap());
    assert!(!scene.is_exiting(plain) && !scene.is_exiting(instant));
}

#[test]
fn an_exit_animates_to_its_values_and_reports_the_node_once_when_done() {
    let mut scene = Scene::new();
    let node = scene.create(Element::Rect);
    scene.set_exit(node, Some(fade(100))).unwrap();
    assert!(scene.begin_exit(node).unwrap());
    assert!(scene.is_exiting(node));
    assert!(
        scene.begin_exit(node).unwrap(),
        "leaving already is leaving"
    );
    let frame = scene.tick_animations(Duration::from_millis(50)).unwrap();
    assert!(frame.exited.is_empty());
    assert!(frame.active);
    assert!((scene.number(node, "opacity").unwrap() - 0.5).abs() < 1e-6);
    assert!((scene.number(node, "scale").unwrap() - 0.75).abs() < 1e-6);
    let frame = scene.tick_animations(Duration::from_millis(60)).unwrap();
    assert_eq!(frame.exited, vec![node]);
    assert_eq!(scene.number(node, "opacity").unwrap(), 0.0);
    let frame = scene.tick_animations(Duration::from_millis(16)).unwrap();
    assert!(frame.exited.is_empty(), "reported once");
    assert!(
        scene.is_exiting(node),
        "out of the flow until its owner removes it"
    );
    scene.remove(node).unwrap();
    assert_eq!(scene.exiting_nodes().count(), 0);
}

#[test]
fn a_node_taken_back_goes_back_to_where_it_was_aimed() {
    let mut scene = Scene::new();
    let node = scene.create(Element::Rect);
    scene.assign(node, "opacity", 0.8).unwrap();
    scene.set_exit(node, Some(fade(100))).unwrap();
    scene.begin_exit(node).unwrap();
    scene.tick_animations(Duration::from_millis(50)).unwrap();
    let midway = scene.number(node, "opacity").unwrap();
    assert!((midway - 0.4).abs() < 1e-6);
    assert!(scene.cancel_exit(node).unwrap());
    assert!(!scene.is_exiting(node));
    assert!(!scene.cancel_exit(node).unwrap());
    // From where it was, not a jump: the first tick moves a little.
    let frame = scene.tick_animations(Duration::from_millis(10)).unwrap();
    assert!(frame.exited.is_empty());
    let after = scene.number(node, "opacity").unwrap();
    assert!(after > midway && after < 0.8, "{after}");
    scene.tick_animations(Duration::from_millis(100)).unwrap();
    assert_eq!(scene.number(node, "opacity").unwrap(), 0.8);
    assert_eq!(scene.number(node, "scale").unwrap(), 1.0);
}

#[test]
fn a_leaving_node_is_a_layout_change_and_keeps_the_box_it_is_given() {
    let mut scene = Scene::new();
    let parent = scene.create(Element::Column);
    let node = scene.create(Element::Rect);
    scene.reparent(node, Some(parent)).unwrap();
    scene.set_exit(node, Some(fade(100))).unwrap();
    let before = scene.layout_revision_of(parent);
    scene.begin_exit(node).unwrap();
    assert!(scene.layout_revision_of(parent) > before);
    assert_eq!(scene.exit_frame(node), None, "no layout has fixed it yet");
    scene.fix_exit_frame(node, [4.0, 8.0, 20.0, 10.0]);
    scene.fix_exit_frame(node, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(scene.exit_frame(node), Some([4.0, 8.0, 20.0, 10.0]));
    // An exit that moves `y` slides the box it keeps.
    scene.assign(node, "y", 6.0).unwrap();
    assert_eq!(scene.exit_frame(node), Some([4.0, 14.0, 20.0, 10.0]));
}

#[test]
fn reduced_motion_ends_an_exit_on_the_next_tick() {
    let mut scene = Scene::new();
    let node = scene.create(Element::Rect);
    scene.set_motion_scale(0.0);
    scene.set_exit(node, Some(fade(300))).unwrap();
    assert!(scene.begin_exit(node).unwrap());
    let frame = scene.tick_animations(Duration::from_millis(16)).unwrap();
    assert_eq!(frame.exited, vec![node]);
}
