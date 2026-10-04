//! Tests for the dock: splitting, joining, folding, walking and floating.

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

fn strs(items: &[&str]) -> IpcValue {
    list(items.iter().map(|s| (*s).into()).collect())
}

fn dock() -> Dock {
    let mut d = Dock::new();
    let layout = map(&[
        ("orientation", "horizontal".into()),
        ("ratios", list(vec![0.25.into(), 0.75.into()])),
        (
            "children",
            list(vec![
                map(&[
                    ("id", "left".into()),
                    ("panels", strs(&["files", "search"])),
                ]),
                map(&[("id", "main".into()), ("panels", strs(&["editor"]))]),
            ]),
        ),
    ]);
    d.configure("layout", &layout).unwrap();
    d
}

fn signal<'a>(effects: &'a Effects, name: &str) -> Option<&'a Vec<IpcValue>> {
    effects
        .signals
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, a)| a)
}

#[test]
fn a_tab_dropped_on_an_edge_splits_and_in_the_middle_joins() {
    let mut d = dock();
    d.handle("drag_start", &["search".into()]).unwrap();
    d.handle(
        "drag_over",
        &[
            "main".into(),
            10.0.into(),
            300.0.into(),
            800.0.into(),
            600.0.into(),
        ],
    )
    .unwrap();
    assert_eq!(d.drop, Some(("main".into(), Zone::Left)));
    let e = d.handle("drop", &[]).unwrap();
    assert!(signal(&e, "layout_changed").is_some());
    // The root split runs the same way, so the new stack joins it.
    let Some(Node::Split {
        children, ratios, ..
    }) = &d.root
    else {
        panic!()
    };
    assert_eq!(children.len(), 3);
    assert!((ratios.iter().sum::<f64>() - 1.0).abs() < 1e-9);
    // Into the middle of another stack, it becomes a tab there; the
    // stack it left, empty, goes.
    d.handle("drag_start", &["search".into()]).unwrap();
    d.handle(
        "drag_over",
        &[
            "main".into(),
            400.0.into(),
            300.0.into(),
            800.0.into(),
            600.0.into(),
        ],
    )
    .unwrap();
    d.handle("drop", &[]).unwrap();
    let Some(Node::Split { children, .. }) = &d.root else {
        panic!()
    };
    assert_eq!(children.len(), 2);
    assert_eq!(d.root.as_ref().unwrap().stack_of("search"), Some("main"));
}

#[test]
fn closing_the_last_panel_of_a_stack_folds_the_split() {
    let mut d = dock();
    d.configure("fixed", &strs(&["editor"])).unwrap();
    d.handle("close", &["editor".into()]).unwrap();
    assert!(d.root.as_ref().unwrap().stack_of("editor").is_some());
    d.handle("close", &["files".into()]).unwrap();
    let e = d.handle("close", &["search".into()]).unwrap();
    assert!(signal(&e, "closed").is_some());
    assert!(matches!(d.root, Some(Node::Stack { .. })));
    assert_eq!(d.focused, "main");
}

#[test]
fn keys_walk_tabs_and_stacks_and_float_round_trips() {
    let mut d = dock();
    d.handle("focus_stack", &["left".into()]).unwrap();
    let e = d
        .handle("key", &["Page_Down".into(), "ctrl".into()])
        .unwrap();
    assert!(e.handled);
    assert_eq!(d.current_of("left"), "search");
    d.handle("key", &["F6".into(), "".into()]).unwrap();
    assert_eq!(d.focused, "main");
    d.handle(
        "float",
        &[
            "files".into(),
            10.0.into(),
            10.0.into(),
            300.0.into(),
            200.0.into(),
        ],
    )
    .unwrap();
    assert_eq!(d.floating.len(), 1);
    d.handle("dock", &["files".into(), "main".into(), "bottom".into()])
        .unwrap();
    assert!(d.floating.is_empty());
    assert_eq!(d.panel_count(), 3);
    d.handle("resize", &["split1".into(), 0.into(), 0.4.into()])
        .unwrap();
}
