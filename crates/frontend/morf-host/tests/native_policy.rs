use morf_lua::Runtime;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[test]
fn reload_retires_effects_before_candidate_code_and_failed_load_cannot_revive_them() {
    let authorized = Arc::new(AtomicBool::new(true));
    let globals = Arc::new(AtomicBool::new(false));
    let retired = Arc::new(AtomicUsize::new(0));
    let mut old = Runtime::default();
    let live = authorized.clone();
    old.set_controller_authorizer(Arc::new(move |token| {
        token == "old" && live.load(Ordering::Acquire)
    }))
    .unwrap();
    let gate = globals.clone();
    old.set_recognizer_gate(Arc::new(move || gate.load(Ordering::Acquire)))
        .unwrap();
    let live = authorized.clone();
    let calls = retired.clone();
    old.set_native_policy_retirement(Arc::new(move || {
        live.store(false, Ordering::Release);
        calls.fetch_add(1, Ordering::Relaxed);
    }));
    // The worker does this before constructing/executing either reload kind.
    old.retire_native_policy();
    let mut candidate = Runtime::default();
    candidate.inherit_native_policy(&old).unwrap();
    assert!(
        candidate
            .execute(
                "bad-replacement.lua",
                br#"
        assert(not __morf_prek_authorized("old"))
        assert(not __morf_global_recognizers_enabled())
        error("candidate rejected")
    "#
            )
            .is_err()
    );
    old.execute(
        "last-good.lua",
        br#"assert(not __morf_prek_authorized("old"))"#,
    )
    .unwrap();
    candidate.retire_native_policy();
    assert_eq!(retired.load(Ordering::Relaxed), 2);
    assert!(!authorized.load(Ordering::Acquire));
    assert!(!globals.load(Ordering::Acquire));
}
