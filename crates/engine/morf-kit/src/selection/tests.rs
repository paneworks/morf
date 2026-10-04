//! Tests for the selection: arrows, grids, multiple choice, ranges, typeahead and the radial menu.

use super::*;
use crate::Archetype;

#[test]
fn a_radial_menu_points_by_angle_and_a_flick_activates() {
    let mut s = Selection::new();
    s.configure("count", &4.0.into()).unwrap();
    // Right of centre: the second of four (three o'clock).
    s.handle("point", &[40.0.into(), 0.0.into(), 16.0.into()]).unwrap();
    assert_eq!(s.current, 2);
    let e = s.handle("point_release", &[0.0.into(), 50.0.into(), 16.0.into()]).unwrap();
    assert_eq!(s.current, 3);
    assert!(e.signals.iter().any(|(n, _)| n == "activated"));
    s.handle("point", &[3.0.into(), 3.0.into(), 16.0.into()]).unwrap();
    assert_eq!(s.current, 3);
}

fn selection(settings: &[(&str, IpcValue)]) -> Selection {
    let mut s = Selection::new();
    for (field, value) in settings {
        s.configure(field, value).unwrap();
    }
    s
}

fn key(s: &mut Selection, name: &str, modifiers: &str) -> Effects {
    s.handle(
        "key",
        &[name.into(), modifiers.into(), "".into(), 0.0.into()],
    )
    .unwrap()
}

#[test]
fn arrows_move_the_current_item_and_skip_disabled_ones() {
    let mut tabs = selection(&[
        ("count", 4.0.into()),
        ("current", 1.0.into()),
        (
            "disabled",
            IpcValue::Table(std::sync::Arc::new(IpcTable::List(vec![2i64.into()]))),
        ),
    ]);
    let effects = key(&mut tabs, "Right", "");
    assert!(effects.handled);
    assert_eq!(tabs.current, 3);
    assert_eq!(tabs.selected, [3].into_iter().collect());
    key(&mut tabs, "Right", "");
    assert!(!key(&mut tabs, "Right", "").handled, "no wrap past the end");
    key(&mut tabs, "Home", "");
    assert_eq!(tabs.current, 1);
}

#[test]
fn a_grid_moves_by_rows_and_pages_stop_at_the_edge() {
    let mut grid = selection(&[
        ("count", 12.0.into()),
        ("orientation", "grid".into()),
        ("columns", 4.0.into()),
        ("current", 2.0.into()),
    ]);
    key(&mut grid, "Down", "");
    assert_eq!(grid.current, 6);
    key(&mut grid, "Page_Down", "");
    assert_eq!(grid.current, 12);
}

#[test]
fn multi_mode_toggles_and_selects_all() {
    let mut list = selection(&[("count", 3.0.into()), ("mode", "multi".into())]);
    list.handle("item_pressed", &[1i64.into(), "".into()])
        .unwrap();
    list.handle("item_pressed", &[3i64.into(), "".into()])
        .unwrap();
    assert_eq!(list.selected, [1, 3].into_iter().collect());
    key(&mut list, "a", "ctrl");
    assert_eq!(list.selected.len(), 3);
}

#[test]
fn shift_extends_a_range() {
    let mut list = selection(&[
        ("count", 6.0.into()),
        ("mode", "range".into()),
        ("orientation", "vertical".into()),
    ]);
    list.handle("item_pressed", &[2i64.into(), "".into()])
        .unwrap();
    key(&mut list, "Down", "shift");
    key(&mut list, "Down", "shift");
    assert_eq!(list.selected, [2, 3, 4].into_iter().collect());
}

#[test]
fn typing_finds_an_item_by_its_label() {
    let labels = ["Firefox", "Files", "Terminal", "Thunar"]
        .iter()
        .map(|l| IpcValue::from(*l))
        .collect();
    let mut list = selection(&[
        ("count", 4.0.into()),
        ("orientation", "vertical".into()),
        (
            "labels",
            IpcValue::Table(std::sync::Arc::new(IpcTable::List(labels))),
        ),
    ]);
    list.handle("key", &["t".into(), "".into(), "t".into(), 0.0.into()])
        .unwrap();
    assert_eq!(list.current, 3);
    list.handle("key", &["h".into(), "".into(), "h".into(), 100.0.into()])
        .unwrap();
    assert_eq!(list.current, 4);
    list.handle("key", &["f".into(), "".into(), "f".into(), 5000.0.into()])
        .unwrap();
    assert_eq!(list.current, 1);
    list.handle("key", &["f".into(), "".into(), "f".into(), 5100.0.into()])
        .unwrap();
    assert_eq!(list.current, 2, "a repeated letter cycles");
}

#[test]
fn return_activates_the_current_item() {
    let mut list = selection(&[("count", 2.0.into()), ("current", 2.0.into())]);
    let effects = key(&mut list, "Return", "");
    assert_eq!(effects.signals[0], ("activated".into(), vec![2i64.into()]));
}
