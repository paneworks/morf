use super::*;
use std::sync::atomic::AtomicU64;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "mh-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
    fn gate(&self) -> Arc<Gate> {
        Gate::open_at(&self.0, "HEADLESS-1").unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn neutral(revision: u64) -> Boundary {
    Boundary {
        contacts: 0,
        neutral: true,
        revision,
    }
}
fn prepare_restore(state: &mut State, gate: &Gate) {
    let token = state.take_restore().unwrap();
    state.prepared_restore(&token, gate.remove_inhibition().is_ok());
}

#[test]
fn native_touch_boundary_includes_unhit_contacts_and_survives_vm_state_reset() {
    let mut contacts = Contacts::default();
    contacts.observe(&Event::TouchDown {
        surface: WindowId::Layer(1),
        id: 7,
        x: -100.,
        y: -100.,
        time_ms: None,
    });
    assert!(!contacts.snapshot().neutral);
    assert_eq!(contacts.snapshot().contacts, 1);
    // Unlike hit-tested PointerInput, this tracker is not reset by Lua reload.
    let _fresh_widget_input = crate::host::surfaces::PointerInput::default();
    assert!(!contacts.snapshot().neutral);
    contacts.observe(&Event::TouchUp {
        surface: WindowId::Layer(1),
        id: 7,
        x: -100.,
        y: -100.,
        time_ms: None,
    });
    assert!(contacts.snapshot().neutral);
    assert_eq!(contacts.snapshot().revision, 2);
    contacts.observe(&Event::TouchUp {
        surface: WindowId::Layer(1),
        id: 99,
        x: 0.,
        y: 0.,
        time_ms: None,
    });
    assert!(!contacts.snapshot().neutral);
    contacts.observe(&Event::TouchCancel);
    assert!(contacts.snapshot().neutral);
}

#[test]
fn quiesce_requires_ui_ack_and_actual_neutral_boundary_before_activation() {
    let fixture = Fixture::new();
    let gate = fixture.gate();
    let mut state = State::new(gate.clone());
    state
        .request("handoff-quiesce", 1, false, &"a".repeat(32), Duration::ZERO)
        .unwrap();
    assert!(!gate.enabled());
    state.update(neutral(0), true, "native", Duration::ZERO);
    assert_eq!(
        state.status()["phase"],
        "quiescing",
        "persistence must finish before acknowledgement"
    );
    gate.persist_inhibition().unwrap();
    state.update(
        Boundary {
            contacts: 1,
            neutral: false,
            revision: 1,
        },
        true,
        "native",
        Duration::ZERO,
    );
    assert!(
        state
            .request("handoff-activate", 1, false, "unused", Duration::ZERO)
            .is_err()
    );
    state.update(neutral(2), false, "native", Duration::ZERO);
    assert!(
        state
            .request("handoff-activate", 1, false, "unused", Duration::ZERO)
            .is_err()
    );
    state.update(neutral(2), true, "native", Duration::ZERO);
    assert!(
        state
            .request(
                "handoff-activate",
                1,
                false,
                "unused",
                Duration::from_millis(250)
            )
            .is_err()
    );
    state.update(neutral(2), true, "native", Duration::from_millis(250));
    state
        .request(
            "handoff-activate",
            1,
            false,
            "unused",
            Duration::from_millis(250),
        )
        .unwrap();
    assert_eq!(state.status()["phase"], "owned");
    assert!(
        !state.update(neutral(2), true, "native", Duration::from_millis(251)),
        "fresh observations must not create an idle repaint loop"
    );
    assert!(!gate.enabled());
    state.synchronize(false);
    assert_eq!(state.status()["phase"], "lost");
    assert!(!gate.enabled());
}

#[test]
fn confirmed_release_waits_for_neutral_and_marker_survives_restart() {
    let fixture = Fixture::new();
    let gate = fixture.gate();
    let mut state = State::new(gate.clone());
    state
        .request("handoff-quiesce", 1, false, &"a".repeat(32), Duration::ZERO)
        .unwrap();
    gate.persist_inhibition().unwrap();
    drop(state);
    drop(gate);
    let gate = fixture.gate();
    assert!(!gate.enabled());
    let mut state = State::new(gate.clone());
    assert_eq!(state.status()["phase"], "lost");
    state
        .request("handoff-quiesce", 2, false, &"b".repeat(32), Duration::ZERO)
        .unwrap();
    gate.persist_inhibition().unwrap();
    assert!(
        state
            .request("handoff-release", 2, false, "unused", Duration::ZERO)
            .is_err()
    );
    state
        .request("handoff-release", 2, true, &"c".repeat(32), Duration::ZERO)
        .unwrap();
    assert!(!state.authorizes(&"b".repeat(32), true));
    assert!(state.authorizes(&"c".repeat(32), true));
    state.update(
        Boundary {
            contacts: 1,
            neutral: false,
            revision: 1,
        },
        true,
        "native",
        Duration::ZERO,
    );
    assert!(!gate.enabled());
    state.update(neutral(2), true, "native", Duration::ZERO);
    prepare_restore(&mut state, &gate);
    assert!(
        !gate.enabled(),
        "only the UI thread may finish release at its current boundary"
    );
    state.update(
        Boundary {
            contacts: 1,
            neutral: false,
            revision: 3,
        },
        true,
        "native",
        Duration::ZERO,
    );
    assert!(
        !gate.enabled(),
        "new contact before UI completion must keep globals disabled"
    );
    state.update(neutral(2), true, "native", Duration::ZERO);
    assert!(gate.enabled());
    assert_eq!(state.status()["phase"], "native");
    assert!(fixture.gate().enabled());
}

#[test]
fn replaced_inhibition_marker_cannot_be_removed_to_resume_recognizers() {
    let fixture = Fixture::new();
    let gate = fixture.gate();
    let mut state = State::new(gate.clone());
    state
        .request("handoff-quiesce", 1, false, &"a".repeat(32), Duration::ZERO)
        .unwrap();
    gate.persist_inhibition().unwrap();
    let path = gate.marker.lock().unwrap().path.clone();
    fs::rename(&path, path.with_file_name("old")).unwrap();
    fs::write(&path, b"replacement").unwrap();
    state
        .request("handoff-release", 1, true, &"b".repeat(32), Duration::ZERO)
        .unwrap();
    state.update(neutral(1), true, "native", Duration::ZERO);
    prepare_restore(&mut state, &gate);
    assert!(!gate.enabled());
    assert_eq!(state.status()["phase"], "lost");
    assert_eq!(fs::read(&path).unwrap(), b"replacement");
}
