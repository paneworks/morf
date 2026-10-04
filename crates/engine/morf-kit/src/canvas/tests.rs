//! Tests for the canvas: picking, moving, resizing, bands, zoom, wires,
//! polygons and cancelling.

use std::sync::Arc;

use morf_value::IpcTable;

use super::*;
use crate::Archetype;

fn map(entries: &[(&str, IpcValue)]) -> IpcValue {
    IpcValue::Table(Arc::new(IpcTable::Map(
        entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    )))
}

fn canvas() -> Canvas {
    let mut c = Canvas::new();
    c.handle("resize", &[400.0.into(), 300.0.into()]).unwrap();
    let items = list(vec![
        map(&[
            ("id", "a".into()),
            ("x", 10.0.into()),
            ("y", 10.0.into()),
            ("w", 50.0.into()),
            ("h", 30.0.into()),
        ]),
        map(&[
            ("id", "b".into()),
            ("x", 200.0.into()),
            ("y", 100.0.into()),
            ("w", 40.0.into()),
            ("h", 40.0.into()),
        ]),
    ]);
    c.configure("items", &items).unwrap();
    c
}

fn signal<'a>(effects: &'a Effects, name: &str) -> Option<&'a Vec<IpcValue>> {
    effects
        .signals
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, a)| a)
}

fn press(c: &mut Canvas, x: f64, y: f64, button: &str, modifiers: &str) -> Effects {
    c.handle(
        "pressed",
        &[
            x.into(),
            y.into(),
            400.0.into(),
            300.0.into(),
            button.into(),
            modifiers.into(),
        ],
    )
    .unwrap()
}

fn drag(c: &mut Canvas, x: f64, y: f64) -> Effects {
    c.handle(
        "dragged",
        &[x.into(), y.into(), 400.0.into(), 300.0.into(), "".into()],
    )
    .unwrap()
}

#[test]
fn the_selected_box_resizes_by_a_handle_on_the_grid() {
    let mut c = canvas();
    c.configure("resizable", &true.into()).unwrap();
    c.configure("grid", &10.0.into()).unwrap();
    c.configure("snap", &true.into()).unwrap();
    press(&mut c, 30.0, 30.0, "left", "");
    c.handle("released", &[]).unwrap();
    // The south-east corner of a (10, 10, 50, 30): at (60, 40).
    press(&mut c, 60.0, 40.0, "left", "");
    assert_eq!(c.gesture_name(), "resize");
    drag(&mut c, 87.0, 66.0);
    let e = c.handle("released", &[]).unwrap();
    let resized = signal(&e, "resized").unwrap();
    assert_eq!(resized[0], "a".into());
    assert_eq!(
        (resized[3].clone(), resized[4].clone()),
        (80.0.into(), 60.0.into())
    );
    // Too small: held at the minimum.
    press(&mut c, 30.0, 30.0, "left", "");
    c.handle("released", &[]).unwrap();
    press(&mut c, 10.0, 10.0, "left", "");
    drag(&mut c, 200.0, 200.0);
    let e = c.handle("released", &[]).unwrap();
    let resized = signal(&e, "resized").unwrap();
    assert_eq!(resized[3], 8.0.into());
}

#[test]
fn a_press_picks_and_a_drag_moves_on_the_grid() {
    let mut c = canvas();
    c.configure("grid", &10.0.into()).unwrap();
    c.configure("snap", &true.into()).unwrap();
    let e = press(&mut c, 20.0, 20.0, "left", "");
    assert_eq!(
        signal(&e, "selection_changed"),
        Some(&vec![ids(&["a".into()])])
    );
    drag(&mut c, 44.0, 27.0);
    let e = c.handle("released", &[]).unwrap();
    assert_eq!(
        signal(&e, "moved"),
        Some(&vec![ids(&["a".into()]), 20.0.into(), 10.0.into()])
    );
}

#[test]
fn a_band_on_nothing_catches_what_it_crosses_and_shift_adds() {
    let mut c = canvas();
    press(&mut c, 150.0, 5.0, "left", "");
    drag(&mut c, 390.0, 290.0);
    c.handle("released", &[]).unwrap();
    assert_eq!(c.selection, vec!["b".to_owned()]);
    press(&mut c, 30.0, 20.0, "left", "shift");
    c.handle("released", &[]).unwrap();
    assert_eq!(c.selection, vec!["b".to_owned(), "a".to_owned()]);
    let e = c.handle("key", &["Delete".into(), "".into()]).unwrap();
    assert!(e.handled && signal(&e, "deleted").is_some());
}

#[test]
fn ctrl_wheel_zooms_about_the_pointer() {
    let mut c = canvas();
    let before = c.world([100.0, 100.0]);
    let e = c
        .handle(
            "wheel",
            &[
                0.into(),
                (-1).into(),
                0.0.into(),
                0.0.into(),
                100.0.into(),
                100.0.into(),
                "ctrl".into(),
            ],
        )
        .unwrap();
    assert!(signal(&e, "view_changed").is_some());
    assert!((c.zoom[0] - 1.2).abs() < 1e-9);
    let after = c.world([100.0, 100.0]);
    assert!((before[0] - after[0]).abs() < 1e-9 && (before[1] - after[1]).abs() < 1e-9);
    // Plain, it pans.
    c.handle(
        "wheel",
        &[
            0.into(),
            1.into(),
            0.0.into(),
            0.0.into(),
            0.0.into(),
            0.0.into(),
            "".into(),
        ],
    )
    .unwrap();
    assert!(c.origin[1] > after[1] - 100.0 / 1.2);
}

#[test]
fn home_fits_everything_and_the_x_axis_zooms_alone() {
    let mut c = canvas();
    c.handle("key", &["Home".into(), "".into()]).unwrap();
    let v = c.view_box();
    assert!(v[0] <= 10.0 && v[2] >= 240.0 && v[1] <= 10.0 && v[3] >= 140.0);
    c.configure("axes", &"x".into()).unwrap();
    let zy = c.zoom[1];
    c.handle("key", &["plus".into(), "".into()]).unwrap();
    assert_eq!(c.zoom[1], zy);
}

#[test]
fn a_wire_joins_out_to_in_and_dropped_says_where() {
    let mut c = canvas();
    let ports = list(vec![
        map(&[
            ("id", "a.out".into()),
            ("item", "a".into()),
            ("x", 60.0.into()),
            ("y", 25.0.into()),
            ("kind", "out".into()),
        ]),
        map(&[
            ("id", "b.in".into()),
            ("item", "b".into()),
            ("x", 200.0.into()),
            ("y", 120.0.into()),
            ("kind", "in".into()),
        ]),
    ]);
    c.configure("ports", &ports).unwrap();
    press(&mut c, 60.0, 25.0, "left", "");
    assert_eq!(c.gesture_name(), "connect");
    drag(&mut c, 201.0, 121.0);
    assert_eq!(c.connect_to, "b.in");
    let e = c.handle("released", &[]).unwrap();
    assert_eq!(
        signal(&e, "connected"),
        Some(&vec!["a.out".into(), "b.in".into()])
    );
    press(&mut c, 60.0, 25.0, "left", "");
    drag(&mut c, 300.0, 250.0);
    let e = c.handle("released", &[]).unwrap();
    assert!(signal(&e, "connect_dropped").is_some());
}

#[test]
fn a_polygon_is_clicked_out_and_closed_on_its_first_point() {
    let mut c = canvas();
    c.configure("tool", &"polygon".into()).unwrap();
    for (x, y) in [(10.0, 10.0), (100.0, 10.0), (100.0, 100.0)] {
        press(&mut c, x, y, "left", "");
        c.handle("released", &[]).unwrap();
    }
    assert_eq!(c.gesture_name(), "draft");
    let e = press(&mut c, 12.0, 11.0, "left", "");
    let drawn = signal(&e, "drawn").unwrap();
    assert_eq!(drawn[0], "polygon".into());
    assert!(c.draft.is_empty());
}

#[test]
fn escape_cancels_then_clears_then_goes_on() {
    let mut c = canvas();
    press(&mut c, 20.0, 20.0, "left", "");
    c.handle("released", &[]).unwrap();
    let e = c.handle("key", &["Escape".into(), "".into()]).unwrap();
    assert!(e.handled && c.selection.is_empty());
    let e = c.handle("key", &["Escape".into(), "".into()]).unwrap();
    assert!(!e.handled);
    // Tab walks the items and leaves after the last.
    assert!(c.handle("key", &["Tab".into(), "".into()]).unwrap().handled);
    assert!(c.handle("key", &["Tab".into(), "".into()]).unwrap().handled);
    assert!(!c.handle("key", &["Tab".into(), "".into()]).unwrap().handled);
}
