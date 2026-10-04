//! Tests for the range: knobs, sliders, steps, direction, seeking, scales and pairs.

use super::*;
use crate::Archetype;

fn range(settings: &[(&str, IpcValue)]) -> Range {
    let mut range = Range::new();
    for (field, value) in settings {
        range.configure(field, value).unwrap();
    }
    range
}

fn signal<'a>(effects: &'a Effects, name: &str) -> Option<&'a Vec<IpcValue>> {
    effects
        .signals
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, a)| a)
}

#[test]
fn a_knob_turns_by_dragging_up_or_round() {
    let mut knob = range(&[("drag_mode", "vertical".into()), ("value", 0.5.into())]);
    // The press does not jump; a hundred pixels up is half the travel.
    let effects = knob.handle("pressed", &[5.0.into(), 150.0.into(), 40.0.into(), 40.0.into()]).unwrap();
    assert_eq!(signal(&effects, "moved"), None);
    knob.handle("dragged", &[5.0.into(), 50.0.into(), 40.0.into(), 40.0.into()]).unwrap();
    assert!((knob.values[0] - 1.0).abs() < 1e-9);
    let mut dial = range(&[("drag_mode", "angular".into())]);
    // Straight up is the middle of a 270-degree sweep from -135.
    dial.handle("pressed", &[20.0.into(), 0.0.into(), 40.0.into(), 40.0.into()]).unwrap();
    assert!((dial.values[0] - 0.5).abs() < 1e-9);
    let angle = dial.state().into_iter().find(|(k, _)| k == "angle").map(|(_, v)| v);
    assert_eq!(angle, Some(0.0.into()));
}

#[test]
fn a_press_jumps_and_a_drag_follows() {
    let mut slider = range(&[("from", 0.0.into()), ("to", 100.0.into())]);
    let effects = slider
        .handle(
            "pressed",
            &[25.0.into(), 5.0.into(), 100.0.into(), 10.0.into()],
        )
        .unwrap();
    assert_eq!(signal(&effects, "moved"), Some(&vec![25.0.into()]));
    let effects = slider
        .handle(
            "dragged",
            &[75.0.into(), 5.0.into(), 100.0.into(), 10.0.into()],
        )
        .unwrap();
    assert_eq!(signal(&effects, "value_changed"), Some(&vec![75.0.into()]));
    slider.handle("released", &[]).unwrap();
    assert!(slider.dragging.is_none());
}

#[test]
fn steps_snap_and_keys_move_by_them() {
    let mut slider = range(&[
        ("to", 10.0.into()),
        ("step", 1.0.into()),
        ("snap", "always".into()),
    ]);
    slider
        .handle(
            "pressed",
            &[33.0.into(), 0.0.into(), 100.0.into(), 10.0.into()],
        )
        .unwrap();
    assert_eq!(slider.values[0], 3.0);
    assert!(
        slider
            .handle("key", &["Right".into(), "".into()])
            .unwrap()
            .handled
    );
    assert_eq!(slider.values[0], 4.0);
    slider.handle("key", &["End".into(), "".into()]).unwrap();
    assert_eq!(slider.values[0], 10.0);
    slider
        .handle("key", &["Page_Down".into(), "".into()])
        .unwrap();
    assert_eq!(slider.values[0], 9.0);
    assert!(
        !slider
            .handle("key", &["a".into(), "".into()])
            .unwrap()
            .handled
    );
}

#[test]
fn right_to_left_flips_the_drawing_and_the_arrows() {
    let mut slider = range(&[
        ("to", 10.0.into()),
        ("step", 1.0.into()),
        ("mirrored", true.into()),
    ]);
    slider.configure("value", &2.0.into()).unwrap();
    let fields = slider.value_fields();
    assert!(fields.contains(&("visual_position".into(), 0.8.into())));
    slider.handle("key", &["Left".into(), "".into()]).unwrap();
    assert_eq!(slider.values[0], 3.0);
}

#[test]
fn a_vertical_range_runs_bottom_to_top() {
    let mut fader = range(&[("orientation", "vertical".into())]);
    fader
        .handle(
            "pressed",
            &[5.0.into(), 25.0.into(), 10.0.into(), 100.0.into()],
        )
        .unwrap();
    assert!((fader.values[0] - 0.75).abs() < 1e-9);
}

#[test]
fn a_seek_bar_moves_only_its_position_until_release() {
    let mut seek = range(&[("live", false.into())]);
    let effects = seek
        .handle(
            "pressed",
            &[50.0.into(), 0.0.into(), 100.0.into(), 10.0.into()],
        )
        .unwrap();
    assert!(signal(&effects, "moved").is_none());
    assert!(effects.changed.contains(&("position".into(), 0.5.into())));
    let effects = seek.handle("released", &[]).unwrap();
    assert_eq!(signal(&effects, "moved"), Some(&vec![0.5.into()]));
}

#[test]
fn a_logarithmic_range_spaces_decades_evenly() {
    let mut volume = range(&[
        ("from", 1.0.into()),
        ("to", 1000.0.into()),
        ("logarithmic", true.into()),
    ]);
    volume
        .handle(
            "pressed",
            &[50.0.into(), 0.0.into(), 100.0.into(), 10.0.into()],
        )
        .unwrap();
    assert!((volume.values[0] - 1000f64.sqrt()).abs() < 1e-6);
}

#[test]
fn a_pair_moves_the_nearer_handle_and_never_crosses() {
    let mut pair = range(&[
        ("range", true.into()),
        ("first", 0.2.into()),
        ("second", 0.6.into()),
    ]);
    pair.handle(
        "pressed",
        &[70.0.into(), 0.0.into(), 100.0.into(), 10.0.into()],
    )
    .unwrap();
    assert!((pair.values[1] - 0.7).abs() < 1e-9);
    pair.handle(
        "dragged",
        &[10.0.into(), 0.0.into(), 100.0.into(), 10.0.into()],
    )
    .unwrap();
    assert!((pair.values[1] - 0.2).abs() < 1e-9, "{:?}", pair.values);
}

#[test]
fn an_angle_wraps_round() {
    let mut angle = range(&[
        ("to", 360.0.into()),
        ("wrap", true.into()),
        ("step", 10.0.into()),
    ]);
    angle.configure("value", &350.0.into()).unwrap();
    angle.handle("key", &["Right".into(), "".into()]).unwrap();
    assert_eq!(angle.values[0], 0.0);
}

#[test]
fn the_wheel_steps_down_when_turned_down() {
    let mut slider = range(&[("to", 10.0.into()), ("step", 1.0.into())]);
    slider.configure("value", &5.0.into()).unwrap();
    slider.handle("wheel", &[0.0.into(), 1.0.into()]).unwrap();
    assert_eq!(slider.values[0], 4.0);
}
