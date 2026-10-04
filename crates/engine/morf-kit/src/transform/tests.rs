//! Tests for the transform: moving, resizing, bounds, maximizing and keys.

use super::*;
use crate::Archetype;

fn boxed() -> Transform {
    let mut t = Transform::new();
    for (f, v) in [
        ("x", 100.0),
        ("y", 100.0),
        ("width", 200.0),
        ("height", 100.0),
    ] {
        t.configure(f, &v.into()).unwrap();
    }
    t
}

#[test]
fn the_body_moves_and_a_corner_resizes_keeping_the_aspect_with_shift() {
    let mut t = boxed();
    t.handle(
        "pressed",
        &[150.0.into(), 150.0.into(), "body".into(), "".into()],
    )
    .unwrap();
    t.handle("dragged", &[170.0.into(), 160.0.into(), "".into()])
        .unwrap();
    let e = t.handle("released", &[]).unwrap();
    assert!(e.signals.iter().any(|(n, _)| n == "committed"));
    assert_eq!((t.rect.x, t.rect.y), (120.0, 110.0));
    t.handle(
        "pressed",
        &[320.0.into(), 210.0.into(), "se".into(), "".into()],
    )
    .unwrap();
    t.handle("dragged", &[420.0.into(), 220.0.into(), "shift".into()])
        .unwrap();
    assert_eq!((t.rect.w, t.rect.h), (300.0, 150.0));
    // A west edge keeps the east one where it was.
    t.handle("released", &[]).unwrap();
    let east = t.rect.x + t.rect.w;
    t.handle(
        "pressed",
        &[120.0.into(), 180.0.into(), "w".into(), "".into()],
    )
    .unwrap();
    t.handle("dragged", &[160.0.into(), 180.0.into(), "".into()])
        .unwrap();
    assert_eq!(t.rect.x + t.rect.w, east);
}

#[test]
fn bounds_and_minimum_hold_and_maximize_round_trips() {
    let mut t = boxed();
    t.configure(
        "bounds",
        &list(vec![0.0.into(), 0.0.into(), 400.0.into(), 300.0.into()]),
    )
    .unwrap();
    t.handle(
        "pressed",
        &[150.0.into(), 150.0.into(), "body".into(), "".into()],
    )
    .unwrap();
    t.handle("dragged", &[950.0.into(), 950.0.into(), "".into()])
        .unwrap();
    assert_eq!((t.rect.x, t.rect.y), (200.0, 200.0));
    t.handle("released", &[]).unwrap();
    t.handle(
        "pressed",
        &[300.0.into(), 300.0.into(), "se".into(), "".into()],
    )
    .unwrap();
    t.handle("dragged", &[0.0.into(), 0.0.into(), "".into()])
        .unwrap();
    assert_eq!((t.rect.w, t.rect.h), (16.0, 16.0));
    t.handle("released", &[]).unwrap();
    t.handle("container", &[800.0.into(), 600.0.into()])
        .unwrap();
    let before = t.rect;
    t.handle("key", &["Return".into(), "".into()]).unwrap();
    assert!(t.maximized && t.rect.w == 800.0);
    t.handle("restore", &[]).unwrap();
    assert_eq!(t.rect, before);
}

#[test]
fn keys_move_resize_and_turn() {
    let mut t = boxed();
    t.configure("rotatable", &true.into()).unwrap();
    t.handle("key", &["Right".into(), "shift".into()]).unwrap();
    assert_eq!(t.rect.x, 110.0);
    t.handle("key", &["Down".into(), "ctrl".into()]).unwrap();
    assert_eq!(t.rect.h, 101.0);
    t.handle("key", &["Right".into(), "alt+shift".into()])
        .unwrap();
    assert_eq!(t.angle, 15.0);
}
