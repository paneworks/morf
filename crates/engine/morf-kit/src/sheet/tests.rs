use super::*;

fn sheet() -> Sheet {
    let mut s = Sheet::new();
    s.configure("rows", &100.0.into()).unwrap();
    s.configure("columns", &5.0.into()).unwrap();
    s
}

fn key(s: &mut Sheet, name: &str, modifiers: &str, typed: &str) -> Effects {
    s.handle("key", &[name.into(), modifiers.into(), typed.into()])
        .unwrap()
}

#[test]
fn the_keys_walk_cells_and_shift_selects_a_range() {
    let mut s = sheet();
    key(&mut s, "Right", "", "");
    key(&mut s, "Down", "shift", "");
    key(&mut s, "Right", "shift", "");
    assert_eq!(s.range(), (1, 2, 2, 3));
    key(&mut s, "End", "ctrl", "");
    assert_eq!(s.at, (100, 5));
    key(&mut s, "Tab", "", "");
    assert_eq!(s.at, (100, 5), "Tab at the last cell goes on out");
    let e = key(&mut s, "c", "ctrl", "");
    assert!(e.signals.iter().any(|(n, _)| n == "copy"));
}

#[test]
fn typing_starts_an_edit_and_return_commits_moving_down() {
    let mut s = sheet();
    let e = key(&mut s, "7", "", "7");
    assert!(s.editing);
    assert_eq!(
        e.signals
            .iter()
            .find(|(n, _)| n == "edit_started")
            .unwrap()
            .1[2],
        "7".into()
    );
    let e = s.handle("commit", &["72".into()]).unwrap();
    assert!(
        e.signals
            .iter()
            .any(|(n, a)| n == "edited" && a[2] == "72".into())
    );
    assert_eq!(s.at, (2, 1));
    key(&mut s, "F2", "", "");
    assert!(key(&mut s, "Escape", "", "").handled);
    assert!(!s.editing);
}

#[test]
fn a_step_sequencer_toggles_instead_of_editing() {
    let mut s = sheet();
    s.configure("toggle", &true.into()).unwrap();
    let e = s
        .handle("pressed", &[3.0.into(), 4.0.into(), "".into()])
        .unwrap();
    assert!(e.signals.iter().any(|(n, _)| n == "toggled"));
    let e = key(&mut s, "space", "", " ");
    assert!(e.signals.iter().any(|(n, _)| n == "toggled"));
    assert!(!s.editing);
}
