use std::collections::HashSet;
use std::time::Duration;

use morf_scene::{Behavior, Easing, Element, ExitSpec, Motion, NodeHandle, Scene, Value};

use super::*;
use crate::animation::behaviors;

const FRAME: Duration = Duration::from_millis(16);

/// A scene, and a property it refuses to have aimed, to stand for a
/// retarget that fails.
struct Host {
    scene: Scene,
    refuse: Option<&'static str>,
}

impl ClaimsHost for Host {
    fn scene(&self) -> &Scene {
        &self.scene
    }

    fn retarget(
        &mut self,
        node: NodeHandle,
        property: &str,
        value: Value,
        motion: Option<Motion>,
    ) -> Result<(), String> {
        if self.refuse == Some(property) {
            return Err(format!("`{property}` refused"));
        }
        self.scene
            .retarget(node, property, value, motion)
            .map_err(|error| error.to_string())
    }
}

fn host() -> (Host, NodeHandle) {
    let mut scene = Scene::default();
    let node = scene.create(Element::Rect);
    (
        Host {
            scene,
            refuse: None,
        },
        node,
    )
}

fn spring() -> Motion {
    Motion::Physics(behaviors::spring(1.0, 14.0, 170.0, 0.001))
}

fn tween(milliseconds: u64, easing: Easing) -> Motion {
    Motion::Tween(Behavior::timed(Duration::from_millis(milliseconds), easing))
}

fn spec(name: &str, class: Priority, holds: bool, x: f64, motion: Option<Motion>) -> ClaimSpec {
    ClaimSpec {
        name: name.to_owned(),
        class,
        motion,
        holds,
        targets: vec![("x".to_owned(), Some(Value::Number(x)))],
    }
}

/// One frame as the runtime runs it: the evaluator, then the scene.
fn frame(claims: &mut Claims, host: &mut Host) -> ClaimsFrame {
    let frame = claims.evaluate(host, FRAME);
    host.scene.tick_animations(FRAME).unwrap();
    frame
}

fn x(host: &Host, node: NodeHandle) -> f64 {
    host.scene.number(node, "x").unwrap()
}

#[test]
fn the_highest_class_wins_and_ties_go_to_the_claim_declared_first() {
    let (mut host, node) = host();
    let mut claims = Claims::default();
    let ids = claims
        .register(
            &host.scene,
            node,
            vec![
                spec("zeta", Priority::Ambient, true, 10.0, None),
                spec("state", Priority::State, false, 90.0, None),
            ],
        )
        .unwrap();
    // A later registration in the same class loses the tie, whatever its name.
    claims
        .register(
            &host.scene,
            node,
            vec![spec("alpha", Priority::Ambient, true, 20.0, None)],
        )
        .unwrap();
    claims.settle_node(&mut host, node);
    assert_eq!(x(&host, node), 10.0, "zeta was declared before alpha");
    claims.set_holds(ids[0], true);
    claims.set_holds(ids[1], true);
    frame(&mut claims, &mut host);
    assert_eq!(x(&host, node), 90.0, "state outranks ambient");
}

#[test]
fn an_asked_claim_is_released_back_to_the_claim_beneath_it() {
    let (mut host, node) = host();
    let mut claims = Claims::default();
    let ids = claims
        .register(
            &host.scene,
            node,
            vec![
                spec("peek", Priority::Asked, false, -40.0, None),
                spec("rest", Priority::Ambient, true, -288.0, None),
            ],
        )
        .unwrap();
    claims.settle_node(&mut host, node);
    assert_eq!(x(&host, node), -288.0);
    claims.set_asked(ids[0], true).unwrap();
    frame(&mut claims, &mut host);
    assert_eq!(x(&host, node), -40.0);
    claims.set_asked(ids[0], false).unwrap();
    frame(&mut claims, &mut host);
    assert_eq!(x(&host, node), -288.0);
    assert!(
        claims.set_asked(ids[1], true).is_err(),
        "only asked claims are asked"
    );
}

/// Asks for a claim moving by `first`, lets it travel part way, then
/// raises a state claim moving by `second`, and checks the hand-over.
fn preempt(first: Motion, second: Motion) {
    let (mut host, node) = host();
    let mut claims = Claims::default();
    let ids = claims
        .register(
            &host.scene,
            node,
            vec![
                spec("peek", Priority::Asked, false, 100.0, Some(first)),
                spec("open", Priority::State, false, -200.0, Some(second)),
                spec("rest", Priority::Ambient, true, 0.0, None),
            ],
        )
        .unwrap();
    claims.settle_node(&mut host, node);
    claims.set_asked(ids[0], true).unwrap();
    for _ in 0..6 {
        frame(&mut claims, &mut host);
    }
    let before = x(&host, node);
    let speed = host.scene.velocity(node, "x").unwrap();
    assert!(before > 1.0 && before < 99.0, "mid-flight at {before}");
    assert!(speed > 10.0, "moving at {speed}");

    claims.set_holds(ids[1], true);
    claims.evaluate(&mut host, Duration::ZERO);
    assert_eq!(x(&host, node), before, "the hand-over moves nothing");
    let carried = host.scene.velocity(node, "x").unwrap();
    assert!(
        (carried - speed).abs() <= speed.abs() * 0.05,
        "the speed carries across: {speed} -> {carried}"
    );
    host.scene.tick_animations(FRAME).unwrap();
    let after = x(&host, node);
    assert!(
        (after - before).abs() < 0.1 * (before + 200.0).abs(),
        "the next frame goes on from there: {before} -> {after}"
    );
    assert!(host.scene.velocity(node, "x").unwrap().is_finite());
    assert_eq!(
        host.scene.target(node, "x").unwrap(),
        &Value::Number(-200.0)
    );
    for _ in 0..200 {
        frame(&mut claims, &mut host);
    }
    assert!((x(&host, node) + 200.0).abs() < 1e-6, "it arrives");
}

#[test]
fn preemption_hands_a_spring_to_a_spring_continuously() {
    preempt(
        spring(),
        Motion::Physics(behaviors::spring(1.0, 26.0, 300.0, 0.001)),
    );
}

#[test]
fn preemption_hands_a_tween_to_a_spring_continuously() {
    preempt(tween(400, Easing::OutCubic), spring());
}

#[test]
fn preemption_hands_a_spring_to_a_tween_continuously() {
    preempt(spring(), tween(300, Easing::InOutQuad));
}

#[test]
fn each_claim_travels_by_its_own_motion() {
    let (mut host, node) = host();
    let mut claims = Claims::default();
    let ids = claims
        .register(
            &host.scene,
            node,
            vec![
                spec(
                    "slow",
                    Priority::State,
                    false,
                    100.0,
                    Some(tween(1000, Easing::Linear)),
                ),
                spec(
                    "fast",
                    Priority::Asked,
                    false,
                    100.0,
                    Some(tween(50, Easing::Linear)),
                ),
                spec("rest", Priority::Ambient, true, 0.0, None),
            ],
        )
        .unwrap();
    claims.settle_node(&mut host, node);
    claims.set_holds(ids[0], true);
    for _ in 0..5 {
        frame(&mut claims, &mut host);
    }
    let x_now = x(&host, node);
    assert!(
        (x_now - 8.0).abs() < 0.5,
        "the slow claim's own tween: {x_now}"
    );
}

#[test]
fn presence_follows_the_easing_of_its_properties() {
    for motion in [tween(240, Easing::OutCubic), spring()] {
        let (mut host, node) = host();
        let mut claims = Claims::default();
        let ids = claims
            .register(
                &host.scene,
                node,
                vec![
                    spec("open", Priority::State, false, 100.0, Some(motion)),
                    spec("rest", Priority::Ambient, true, 0.0, None),
                ],
            )
            .unwrap();
        claims.settle_node(&mut host, node);
        assert_eq!(claims.presence(ids[0]), 0.0);
        assert_eq!(claims.presence(ids[1]), 1.0);
        claims.set_holds(ids[0], true);
        let mut eased = false;
        for _ in 0..120 {
            frame(&mut claims, &mut host);
            let presence = claims.presence(ids[0]);
            let progress = x(&host, node) / 100.0;
            assert!(
                (presence - progress).abs() < 2e-3,
                "{motion:?}: presence {presence} against progress {progress}"
            );
            eased |= presence > 0.05 && presence < 0.95;
        }
        assert!(eased, "it passes between the ends");
        assert_eq!(claims.presence(ids[0]), 1.0);
        assert_eq!(claims.presence(ids[1]), 0.0);
        assert!(!claims.wants_tick(), "nothing left to do");
    }
}

#[test]
fn registration_is_all_or_nothing() {
    let (host, node) = host();
    let mut claims = Claims::default();
    let mut bad = spec("bad", Priority::State, true, 0.0, None);
    bad.targets
        .push(("color".to_owned(), Some(Value::String("hello".into()))));
    let error = claims
        .register(
            &host.scene,
            node,
            vec![spec("good", Priority::Ambient, true, 5.0, None), bad],
        )
        .unwrap_err();
    assert!(error.contains("bad"), "{error}");
    assert!(claims.is_empty() && !claims.is_claimed(node, "x"));
    let spring_on_text = ClaimSpec {
        targets: vec![("id".to_owned(), Some(Value::String("a".into())))],
        ..spec("text", Priority::State, true, 0.0, Some(spring()))
    };
    assert!(
        claims
            .register(&host.scene, node, vec![spring_on_text])
            .is_err()
    );
}

#[test]
fn one_failing_property_leaves_the_rest_moving() {
    let (mut host, node) = host();
    host.refuse = Some("y");
    let mut claims = Claims::default();
    let mut both = spec("both", Priority::State, true, 50.0, None);
    both.targets
        .push(("y".to_owned(), Some(Value::Number(9.0))));
    claims.register(&host.scene, node, vec![both]).unwrap();
    let frame = claims.evaluate(&mut host, FRAME);
    assert_eq!(frame.errors.len(), 1, "{:?}", frame.errors);
    assert_eq!(x(&host, node), 50.0);
}

#[test]
fn removing_claims_hands_the_property_back() {
    let (mut host, node) = host();
    let mut claims = Claims::default();
    let ids = claims
        .register(
            &host.scene,
            node,
            vec![
                spec("open", Priority::State, true, 30.0, None),
                spec("rest", Priority::Ambient, true, 0.0, None),
            ],
        )
        .unwrap();
    claims.settle_node(&mut host, node);
    assert_eq!(x(&host, node), 30.0);
    claims.remove(ids[0]).unwrap();
    frame(&mut claims, &mut host);
    assert_eq!(x(&host, node), 0.0, "rest wins without open");
    claims.forget(&HashSet::from([node]));
    assert!(claims.is_empty() && !claims.is_claimed(node, "x"));
    assert!(!claims.wants_tick());
}

#[test]
fn an_exit_keeps_the_properties_it_moves_until_it_is_taken_back() {
    let (mut host, node) = host();
    host.scene
        .set_exit(
            node,
            Some(ExitSpec {
                values: vec![("opacity".to_owned(), Value::Number(0.0))],
                behavior: Behavior::timed(Duration::from_millis(200), Easing::Linear),
            }),
        )
        .unwrap();
    let mut claims = Claims::default();
    let opacity = |value: f64| vec![("opacity".to_owned(), Some(Value::Number(value)))];
    let ids = claims
        .register(
            &host.scene,
            node,
            vec![
                ClaimSpec {
                    targets: opacity(0.9),
                    ..spec("lit", Priority::State, false, 0.0, None)
                },
                ClaimSpec {
                    targets: opacity(0.5),
                    ..spec("dim", Priority::Ambient, true, 0.0, None)
                },
            ],
        )
        .unwrap();
    claims.settle_node(&mut host, node);
    assert!(host.scene.begin_exit(node).unwrap());
    claims.set_holds(ids[0], true);
    frame(&mut claims, &mut host);
    assert_eq!(
        host.scene.target(node, "opacity").unwrap(),
        &Value::Number(0.0)
    );
    host.scene.cancel_exit(node).unwrap();
    claims.touch(node);
    frame(&mut claims, &mut host);
    assert_eq!(
        host.scene.target(node, "opacity").unwrap(),
        &Value::Number(0.9)
    );
}

#[test]
fn unclaimed_properties_are_untouched() {
    let (mut host, node) = host();
    host.scene.assign(node, "y", 7.0).unwrap();
    let mut claims = Claims::default();
    claims
        .register(
            &host.scene,
            node,
            vec![spec("open", Priority::State, true, 30.0, None)],
        )
        .unwrap();
    frame(&mut claims, &mut host);
    assert_eq!(host.scene.target(node, "y").unwrap(), &Value::Number(7.0));
    assert!(!claims.is_claimed(node, "y"));
}
