//! Motion with no VM: loops start, keep, restart and end; follows track
//! their source; theme colours ease; `on_finished` handlers are owed and
//! called; exits hold a node in retention; and the settings behaviours and
//! flings are built from are checked as a configuration writes them.

use std::any::Any;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use morf_scene::retain::Retention;
use morf_scene::{Behavior, Color, Easing, Element, ExitSpec, Physics, Repeat};

use super::*;
use crate::{HandlerId, HandlerRegistry};

struct Nowhere;

impl HandlerRegistry for Nowhere {
    fn release(&self, _: HandlerId) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn handler(id: u64) -> Handler {
    Handler::new(HandlerId(id), Rc::new(Nowhere))
}

#[derive(Default)]
struct Host {
    scene: Scene,
    animation: Animation,
}

impl AnimationHost for Host {
    fn animation(&mut self) -> &mut Animation {
        &mut self.animation
    }
    fn scene(&mut self) -> &mut Scene {
        &mut self.scene
    }
    fn assign(&mut self, node: NodeHandle, property: &str, value: Value) -> Result<(), String> {
        self.scene
            .assign(node, property, value)
            .map_err(|error| error.to_string())
    }
}

fn host_with_item() -> (Host, NodeHandle) {
    let mut host = Host::default();
    let node = host.scene.create(Element::Item);
    (host, node)
}

fn map(fields: &[(&str, Value)]) -> Value {
    Value::Map(
        fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn opacity_loop(extra: &[(&str, Value)]) -> Value {
    let mut fields = vec![
        ("from", Value::Number(0.2)),
        ("to", Value::Number(1.0)),
        ("duration", Value::Number(100.0)),
    ];
    fields.extend_from_slice(extra);
    map(&[("opacity", map(&fields))])
}

#[test]
fn a_loop_runs_until_it_is_taken_away_and_puts_its_property_back() {
    let (mut host, node) = host_with_item();
    loops::apply_loops(&mut host, node, &opacity_loop(&[])).unwrap();
    assert!(host.scene.is_animating(node, "opacity").unwrap());
    host.scene
        .tick_animations(Duration::from_millis(250))
        .unwrap();
    assert!(
        host.scene.is_animating(node, "opacity").unwrap(),
        "a loop is forever"
    );
    loops::apply_loops(&mut host, node, &Value::Nil).unwrap();
    assert!(!host.scene.is_animating(node, "opacity").unwrap());
    assert_eq!(host.scene.number(node, "opacity").unwrap(), 0.2);
    assert!(host.animation.loops.is_empty());
}

#[test]
fn a_held_loop_ends_where_the_motion_had_it() {
    let (mut host, node) = host_with_item();
    loops::apply_loops(
        &mut host,
        node,
        &opacity_loop(&[("hold", Value::Bool(true))]),
    )
    .unwrap();
    host.scene.tick_animations(Duration::ZERO).unwrap();
    host.scene
        .tick_animations(Duration::from_millis(50))
        .unwrap();
    let moving = host.scene.number(node, "opacity").unwrap();
    assert!(moving > 0.2 && moving < 1.0, "half way it was at {moving}");
    loops::apply_loops(&mut host, node, &Value::Nil).unwrap();
    assert!((host.scene.number(node, "opacity").unwrap() - moving).abs() < 1e-9);
}

#[test]
fn the_same_loop_again_is_left_running_and_a_new_one_restarts() {
    let (mut host, node) = host_with_item();
    let spec = opacity_loop(&[]);
    loops::apply_loops(&mut host, node, &spec).unwrap();
    host.scene.tick_animations(Duration::ZERO).unwrap();
    host.scene
        .tick_animations(Duration::from_millis(40))
        .unwrap();
    let before = host.scene.animation_progress(node, "opacity").unwrap();
    loops::apply_loops(&mut host, node, &spec).unwrap();
    assert_eq!(
        host.scene.animation_progress(node, "opacity").unwrap(),
        before
    );
    loops::apply_loops(
        &mut host,
        node,
        &opacity_loop(&[("delay", Value::Number(5.0))]),
    )
    .unwrap();
    assert_ne!(
        host.scene.animation_progress(node, "opacity").unwrap(),
        before
    );
}

#[test]
fn a_loop_says_what_is_wrong_with_it() {
    let (mut host, node) = host_with_item();
    let wrong = |host: &mut Host, fields: &[(&str, Value)]| {
        loops::apply_loops(host, node, &map(&[("opacity", map(fields))])).unwrap_err()
    };
    assert_eq!(
        wrong(&mut host, &[("duration", Value::Number(10.0))]),
        "loop `opacity` needs a `to`"
    );
    assert_eq!(
        wrong(
            &mut host,
            &[("to", Value::Number(1.0)), ("duration", Value::Number(0.0))]
        ),
        "loop `opacity` needs a duration"
    );
    assert_eq!(
        wrong(
            &mut host,
            &[("to", Value::Number(1.0)), ("speed", Value::Number(1.0))]
        ),
        "loop `opacity` has no field `speed`"
    );
    assert_eq!(
        loops::apply_loops(&mut host, node, &Value::Number(1.0)).unwrap_err(),
        "loop must be a property-keyed table"
    );
}

#[test]
fn a_follow_tracks_its_source_clamped_and_goes_with_it() {
    let (mut host, target) = host_with_item();
    let source = host.scene.create(Element::Item);
    host.scene.assign(source, "x", 40.0).unwrap();
    host.animation.follows.push(Follow {
        target,
        property: "x".to_owned(),
        source,
        source_property: "x".to_owned(),
        scale: 2.0,
        offset: 10.0,
        min: 0.0,
        max: 60.0,
    });
    assert_eq!(follow::apply_follows(&mut host), 1);
    assert_eq!(
        host.scene.number(target, "x").unwrap(),
        60.0,
        "clamped to max"
    );
    assert_eq!(follow::apply_follows(&mut host), 0, "nothing moved since");
    host.scene.assign(source, "x", 5.0).unwrap();
    assert_eq!(follow::apply_follows(&mut host), 1);
    assert_eq!(host.scene.number(target, "x").unwrap(), 20.0);
    host.scene.remove(source).unwrap();
    follow::apply_follows(&mut host);
    assert!(host.animation.follows.is_empty());
}

#[test]
fn a_theme_colour_eases_and_is_dropped_once_there() {
    let mut graph = morf_scene::reactive::Graph::<IpcValue>::new(8);
    let signal = graph.signal("accent", IpcValue::String(String::new()));
    let mut fades = vec![ThemeFade {
        signal,
        from: Color::rgba8(0, 0, 0, 255),
        to: Color::rgba8(255, 255, 255, 255),
        elapsed: Duration::ZERO,
        duration: Duration::from_millis(100),
        easing: Easing::Linear,
    }];
    let half = fades::advance(&mut fades, Duration::from_millis(50));
    assert_eq!(half.len(), 1);
    assert!(half[0].1.red > 0.0 && half[0].1.red < 1.0);
    assert_eq!(fades.len(), 1);
    let end = fades::advance(&mut fades, Duration::from_millis(60));
    assert!((end[0].1.blue - 1.0).abs() < 1e-5 && (end[0].1.red - 1.0).abs() < 1e-5);
    assert!(fades.is_empty());
}

fn eased(ms: u64) -> Behavior {
    Behavior {
        duration: Duration::from_millis(ms),
        ..Behavior::default()
    }
}

#[test]
fn a_finished_behaviour_owes_its_handler_the_property_and_why() {
    let (mut host, node) = host_with_item();
    host.scene
        .set_behavior(node, "opacity", Some(eased(100)))
        .unwrap();
    host.animation
        .callbacks
        .insert((node, "opacity".to_owned()), handler(7));
    host.scene.assign(node, "opacity", 0.0).unwrap();
    host.scene.tick_animations(Duration::ZERO).unwrap();
    let frame = host
        .scene
        .tick_animations(Duration::from_millis(200))
        .unwrap();
    let finished = host.animation.finished(&frame);
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].handler.id(), HandlerId(7));
    assert_eq!(finished[0].source, "behavior");
    assert_eq!(
        finished[0].args,
        vec![
            IpcValue::String("opacity".to_owned()),
            IpcValue::String("completed".to_owned())
        ]
    );
    // Still registered: a behaviour finishes every time it moves.
    assert_eq!(host.animation.callbacks.len(), 1);
    let mut removed = std::collections::HashSet::new();
    removed.insert(node);
    host.animation.forget(&removed);
    assert!(host.animation.callbacks.is_empty());
}

struct Calls {
    seen: Vec<(HandlerId, Vec<IpcValue>)>,
    fail: bool,
}

impl Handlers for Calls {
    fn call(&mut self, handler: &Handler, args: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
        self.seen.push((handler.id(), args.to_vec()));
        if self.fail {
            Err("boom".to_owned())
        } else {
            Ok(Vec::new())
        }
    }
}

#[test]
fn every_owed_handler_is_called_and_a_failure_is_only_a_warning() {
    let owed = || {
        vec![
            Finished {
                handler: handler(1),
                args: vec![IpcValue::String("completed".to_owned())],
                source: "animation group",
            },
            Finished {
                handler: handler(2),
                args: Vec::new(),
                source: "behavior",
            },
        ]
    };
    let mut calls = Calls {
        seen: Vec::new(),
        fail: false,
    };
    assert!(report_finished(&mut calls, owed()).is_empty());
    assert_eq!(calls.seen.len(), 2);
    calls.fail = true;
    assert_eq!(
        report_finished(&mut calls, owed()),
        vec![
            "animation group on_finished: boom".to_owned(),
            "behavior on_finished: boom".to_owned()
        ]
    );
}

#[test]
fn an_exit_holds_the_node_and_taking_it_back_lets_go() {
    let (mut host, node) = host_with_item();
    let mut retention = Retention::default();
    assert_eq!(
        exits::begin_exit(&mut host.scene, &mut retention, &mut host.animation, node),
        exits::ExitStart::None,
        "no exit declared: it goes at once"
    );
    host.scene
        .set_exit(
            node,
            Some(ExitSpec {
                values: vec![("opacity".to_owned(), Value::Number(0.0))],
                behavior: eased(100),
            }),
        )
        .unwrap();
    let start = exits::begin_exit(&mut host.scene, &mut retention, &mut host.animation, node);
    assert_eq!(start, exits::ExitStart::Started);
    assert!(retention.state(node).is_some());
    assert!(host.animation.exit_registered.contains(&node));
    assert_eq!(
        exits::begin_exit(&mut host.scene, &mut retention, &mut host.animation, node),
        exits::ExitStart::Already
    );
    assert!(exits::cancel_exit(
        &mut host.scene,
        &mut retention,
        &mut host.animation,
        node
    ));
    assert!(retention.state(node).is_none());
    assert!(!host.animation.exit_registered.contains(&node));
    assert!(!exits::cancel_exit(
        &mut host.scene,
        &mut retention,
        &mut host.animation,
        node
    ));
}

#[test]
fn behaviour_settings_are_checked_as_written() {
    use behaviors::Loops;
    assert_eq!(behaviors::repeat(Loops::Absent, false), Ok(Repeat::Once));
    assert_eq!(behaviors::repeat(Loops::Absent, true), Ok(Repeat::PingPong));
    assert_eq!(
        behaviors::repeat(Loops::Count(3.0), true),
        Ok(Repeat::PingPongTimes(3))
    );
    assert_eq!(
        behaviors::repeat(Loops::Count(0.0), false),
        Err("behavior loops must be at least one pass".to_owned())
    );
    assert_eq!(
        behaviors::repeat(Loops::Name("twice".to_owned()), false),
        Err("unknown behavior loops mode `twice`".to_owned())
    );
    assert_eq!(behaviors::retarget(None), Ok(true));
    assert_eq!(behaviors::retarget(Some("restart")), Ok(false));
    assert!(behaviors::duration(-1.0).is_err());
    assert_eq!(behaviors::delay(250.0), Ok(Duration::from_millis(250)));
    assert!(behaviors::time_scale(0.0).is_err());
    assert!(behaviors::rotation_direction(Some("sideways")).is_err());
    assert!(behaviors::color_space(Some("cmyk")).is_err());
}

#[test]
fn a_fling_takes_a_preset_and_both_bounds_or_neither() {
    assert_eq!(fling::preset_or_default(None), Ok((1400.0, 2.0)));
    assert_eq!(fling::preset_or_default(Some("snappy")), Ok((3600.0, 4.0)));
    assert!(fling::preset_or_default(Some("wild")).is_err());
    assert_eq!(
        fling::decay(1.0, 2.0, 0.0, 0.0, Some(0.0), None),
        Err("a fling bound needs both `min` and `max`".to_owned())
    );
    let Ok(Physics::Decay {
        bounds,
        restitution,
        ..
    }) = fling::decay(1.0, 2.0, 9.8, 0.5, Some(0.0), Some(10.0))
    else {
        panic!("a fling with both bounds is a decay");
    };
    assert_eq!(bounds, Some((0.0, 10.0)));
    assert_eq!(restitution, 0.5);
}

#[test]
fn an_easing_is_a_name_a_bezier_or_a_spline() {
    assert_eq!(easing::easing_named("out_cubic"), Ok(Easing::OutCubic));
    assert!(easing::easing_named("wobbly").is_err());
    let bezier = Value::List(vec![
        Value::Number(0.2),
        Value::Number(0.0),
        Value::Number(0.0),
        Value::Number(1.0),
    ]);
    assert!(matches!(
        easing::easing_from_scene(&bezier),
        Ok(Easing::CubicBezier { .. })
    ));
    assert_eq!(
        easing::cubic_bezier(1.5, 0.0, 0.0, 1.0),
        Err("easing x1 and x2 must be between 0 and 1".to_owned())
    );
    assert_eq!(easing::easing_from_scene(&Value::Nil), Ok(Easing::Linear));
}
