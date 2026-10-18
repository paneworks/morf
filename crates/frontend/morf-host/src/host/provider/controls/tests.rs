use super::*;

// Positive tests model a root credential using our own live pidfd. Actual
// SO_PEERCRED rejection is exercised separately on the private endpoint.
fn root_model() -> PeerIdentity {
    PeerIdentity {
        pid: std::process::id(),
        uid: 0,
    }
}
fn state() -> Controls {
    Controls::new("a".repeat(32), "HEADLESS-1".into())
}
fn envelope(op: &str) -> Value {
    json!({"version":1,"op":op,"epoch":"a".repeat(32),"ui_generation":"1",
        "fence":{"broker":[1,2],"configuration":1,"session":1}})
}
fn lease(state: &mut Controls, at: u64, nonce: &str) -> Value {
    let mut r = envelope("lease");
    r["ui_generation"] = json!(state.generation.to_string());
    r["expires_ns"] = json!(((at + 1000) * 1_000_000).to_string());
    state
        .request(root_model(), &r, Duration::from_millis(at), nonce)
        .unwrap()
}
fn submit(state: &mut Controls, id: u64, at: u64) -> Value {
    let mut r = envelope("submit");
    r["ui_generation"] = json!(state.generation.to_string());
    r["id"] = json!(id.to_string());
    r["deadline_ns"] = json!(((at + 100) * 1_000_000).to_string());
    r["request"] = json!({"version":1,"output":"HEADLESS-1","epoch":"b".repeat(32),
        "gesture":format!("{id:032x}"),"ordinal":1,"phase":"discrete","action":"apps",
        "dx":0,"dy":0,"vx":0,"vy":0});
    state
        .request(root_model(), &r, Duration::from_millis(at), "unused")
        .unwrap()
}

#[test]
fn model_queued_work_expires_and_receipts_do_not_reexecute() {
    let mut s = state();
    assert_eq!(lease(&mut s, 0, "first")["status"], "accepted");
    assert_eq!(submit(&mut s, 1, 0)["status"], "queued");
    assert!(s.take(Duration::from_millis(100)).is_none());
    assert_eq!(s.results[&1]["error"], "expired");
    assert_eq!(submit(&mut s, 1, 100)["error"], "duplicate");
    assert_eq!(submit(&mut s, 2, 100)["status"], "queued");
    let job = s.take(Duration::from_millis(101)).unwrap();
    s.complete(&job, true);
    assert_eq!(s.results[&2]["status"], "dispatched");
    assert!(s.take(Duration::from_millis(102)).is_none());
}
#[test]
fn model_gesture_cancellation_purges_only_its_jobs_and_retires_active_guards() {
    let mut s = state();
    lease(&mut s, 0, &"c".repeat(32));
    submit(&mut s, 1, 0);
    submit(&mut s, 2, 0);
    let active = s.take(Duration::ZERO).unwrap();
    assert!(s.valid_for_effect(&active.token, Duration::ZERO));
    let mut cancel = envelope("cancel-gesture");
    cancel["gesture_epoch"] = json!("b".repeat(32));
    cancel["gesture"] = json!(format!("{:032x}", 1));
    assert_eq!(
        s.request(root_model(), &cancel, Duration::ZERO, "unused")
            .unwrap()["status"],
        "accepted"
    );
    assert!(!s.valid_for_effect(&active.token, Duration::ZERO));
    assert!(
        s.valid(&"c".repeat(32), Duration::ZERO),
        "gesture cancellation revoked the owning lease"
    );
    s.complete(&active, true);
    assert_eq!(
        s.results[&1]["error"], "retired",
        "late UI completion revived a cancelled job"
    );
    let other = s.take(Duration::ZERO).unwrap();
    assert_eq!(other.id, 2);
    assert!(s.valid_for_effect(&other.token, Duration::ZERO));
    s.complete(&other, true);
    assert_eq!(s.results[&2]["status"], "dispatched");
    assert_eq!(
        s.request(root_model(), &cancel, Duration::ZERO, "unused")
            .unwrap()["status"],
        "accepted"
    );
}
#[test]
fn model_cancelled_pending_jobs_and_foreign_cancellation_cannot_execute() {
    let mut s = state();
    lease(&mut s, 0, "first");
    submit(&mut s, 1, 0);
    let mut cancel = envelope("cancel-gesture");
    cancel["gesture_epoch"] = json!("b".repeat(32));
    cancel["gesture"] = json!(format!("{:032x}", 1));
    let mut foreign = cancel.clone();
    foreign["fence"]["session"] = json!(2);
    assert_eq!(
        s.request(root_model(), &foreign, Duration::ZERO, "unused")
            .unwrap()["error"],
        "stale_fence"
    );
    assert_eq!(s.pending.len(), 1);
    s.request(root_model(), &cancel, Duration::ZERO, "unused")
        .unwrap();
    assert!(s.take(Duration::ZERO).is_none());
    assert_eq!(s.results[&1]["error"], "retired");
    // The retired identity remains rejected even with a fresh request ID.
    let mut replay = envelope("submit");
    replay["id"] = json!("2");
    replay["deadline_ns"] = json!("100000000");
    replay["request"] = json!({"version":1,"output":"HEADLESS-1","epoch":"b".repeat(32),
        "gesture":format!("{:032x}",1),"ordinal":1,"phase":"discrete","action":"apps","dx":0,"dy":0,"vx":0,"vy":0});
    assert_eq!(
        s.request(root_model(), &replay, Duration::ZERO, "unused")
            .unwrap()["error"],
        "retired"
    );
}

#[test]
fn model_revoke_requires_a_new_fence_and_expiry_rotates_guards() {
    let mut s = state();
    lease(&mut s, 0, "first");
    submit(&mut s, 1, 0);
    assert_eq!(
        s.request(root_model(), &envelope("revoke"), Duration::ZERO, "unused")
            .unwrap()["status"],
        "accepted"
    );
    assert!(!s.valid("first", Duration::ZERO));
    assert_eq!(s.results[&1]["error"], "retired");
    assert_eq!(lease(&mut s, 1, "second")["error"], "retired");
    let mut r = envelope("lease");
    r["fence"]["session"] = json!(2);
    r["expires_ns"] = json!("1000000000");
    assert_eq!(
        s.request(root_model(), &r, Duration::ZERO, "second")
            .unwrap()["status"],
        "accepted"
    );
    assert!(!s.valid("first", Duration::ZERO));
    assert!(s.valid("second", Duration::ZERO));
    assert_eq!(lease(&mut s, 0, "third")["error"], "stale_fence");
    let mut s = state();
    lease(&mut s, 0, "first");
    submit(&mut s, 1, 0);
    assert_eq!(lease(&mut s, 1000, "second")["status"], "accepted");
    assert!(!s.valid("first", Duration::from_millis(1000)));
    assert!(s.valid("second", Duration::from_millis(1000)));
    assert!(s.take(Duration::from_millis(1000)).is_none());
    assert_eq!(submit(&mut s, 1, 1000)["error"], "duplicate");
}

#[test]
fn model_ui_reload_retires_queue_and_guards_without_reviving_ids() {
    let mut s = state();
    lease(&mut s, 0, "first");
    submit(&mut s, 1, 0);
    s.reset_ui();
    assert!(!s.valid("first", Duration::ZERO));
    assert_eq!(s.results[&1]["error"], "ui_replaced");
    let mut r = envelope("lease");
    r["expires_ns"] = json!("1000000000");
    assert_eq!(
        s.request(root_model(), &r, Duration::ZERO, "second")
            .unwrap()["error"],
        "ui_replaced"
    );
    assert_eq!(lease(&mut s, 0, "second")["status"], "accepted");
    assert_eq!(submit(&mut s, 1, 0)["error"], "duplicate");
    assert_eq!(s.capabilities()["ui_generation"], "2");
    s.close();
    assert!(!s.valid("second", Duration::ZERO));
    assert!(
        s.request(root_model(), &r, Duration::ZERO, "third")
            .is_none()
    );
}

#[test]
fn model_queue_and_receipt_memory_are_bounded() {
    let mut s = state();
    lease(&mut s, 0, "first");
    for id in 1..=8 {
        assert_eq!(submit(&mut s, id, 0)["status"], "queued");
    }
    for id in 9..=80 {
        assert_eq!(submit(&mut s, id, 0)["error"], "capacity");
    }
    assert_eq!(s.pending.len(), 8);
    assert_eq!(s.results.len(), 64);
    s.invalidate();
    assert!(s.pending.is_empty());
    assert_eq!(s.results.len(), 64);
    assert_eq!(submit(&mut s, 80, 0)["error"], "duplicate");
}

#[test]
fn nonroot_malformed_cross_output_and_overlong_deadlines_are_refused() {
    let mut s = state();
    let mut r = envelope("lease");
    r["expires_ns"] = json!("1000000000");
    assert!(
        s.request(
            PeerIdentity {
                pid: std::process::id(),
                uid: 1000
            },
            &r,
            Duration::ZERO,
            "first"
        )
        .is_none()
    );
    r["expires_ns"] = json!("1000000001");
    assert_eq!(
        s.request(root_model(), &r, Duration::ZERO, "first")
            .unwrap()["error"],
        "invalid_deadline"
    );
    lease(&mut s, 0, "first");
    submit(&mut s, 1, 0);
    let mut payload: Value = serde_json::from_str(&s.pending[0].payload).unwrap();
    payload["output"] = json!("other");
    assert!(!valid_payload(&payload, "HEADLESS-1"));
    payload["output"] = json!("HEADLESS-1");
    payload["dx"] = json!(1_000_001);
    assert!(!valid_payload(&payload, "HEADLESS-1"));
    assert!(decimal(&json!("01")).is_none());
    assert!(decimal(&json!(1)).is_none());
}

#[test]
fn model_issuer_death_retires_queued_work_without_waiting_for_expiry() {
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let peer = PeerIdentity {
        pid: child.id(),
        uid: 0,
    };
    let issuer = Issuer::open(peer).unwrap();
    let mut s = state();
    lease(&mut s, 0, "first");
    submit(&mut s, 1, 0);
    s.issuer = Some(issuer);
    assert!(s.valid("first", Duration::ZERO));
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(!s.valid("first", Duration::ZERO));
    assert!(s.take(Duration::ZERO).is_none());
    assert_eq!(s.results[&1]["error"], "expired");
}

#[test]
fn model_handoff_orders_quiesce_activation_and_confirmed_release() {
    use std::os::unix::fs::DirBuilderExt;
    let path =
        std::env::temp_dir().join(format!("mhc-{}-{}", std::process::id(), nonce().unwrap()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .unwrap();
    let gate = super::super::handoff::Gate::open_at(&path, "HEADLESS-1").unwrap();
    let mut s = Controls::with_handoff("a".repeat(32), "HEADLESS-1".into(), gate.clone());
    lease(&mut s, 0, "lease");
    assert_eq!(submit(&mut s, 1, 0)["error"], "handoff_required");
    let mut quiesce = envelope("handoff-quiesce");
    quiesce["ownership_generation"] = json!("1");
    assert!(
        s.request(
            PeerIdentity {
                pid: std::process::id(),
                uid: 1000
            },
            &quiesce,
            Duration::ZERO,
            &"a".repeat(32)
        )
        .is_none()
    );
    assert!(gate.enabled());
    assert_eq!(
        s.request(root_model(), &quiesce, Duration::ZERO, &"a".repeat(32))
            .unwrap()["status"],
        "accepted"
    );
    assert!(!gate.enabled());
    gate.persist_inhibition().unwrap();
    let mut activate = envelope("handoff-activate");
    activate["ownership_generation"] = json!("1");
    assert_eq!(
        s.request(root_model(), &activate, Duration::ZERO, "unused")
            .unwrap()["error"],
        "not_neutral"
    );
    s.handoff_update(
        &"a".repeat(32),
        super::super::handoff::Boundary {
            contacts: 0,
            neutral: true,
            revision: 2,
        },
        true,
        "native",
        Duration::from_millis(1),
    );
    assert_eq!(
        s.request(root_model(), &activate, Duration::from_millis(1), "unused")
            .unwrap()["status"],
        "accepted"
    );
    assert_eq!(submit(&mut s, 1, 1)["status"], "queued");
    let mut release = envelope("handoff-release");
    release["ownership_generation"] = json!("1");
    release["descriptors_released"] = json!(false);
    assert_eq!(
        s.request(root_model(), &release, Duration::from_millis(1), "unused")
            .unwrap()["error"],
        "release_unconfirmed"
    );
    release["descriptors_released"] = json!(true);
    assert_eq!(
        s.request(
            root_model(),
            &release,
            Duration::from_millis(1),
            &"b".repeat(32)
        )
        .unwrap()["status"],
        "accepted"
    );
    assert_eq!(s.results[&1]["error"], "retired");
    assert!(!s.valid("lease", Duration::from_millis(1)));
    assert!(!s.handoff_authorizes(&"a".repeat(32), true));
    s.handoff_update(
        &"b".repeat(32),
        super::super::handoff::Boundary {
            contacts: 1,
            neutral: false,
            revision: 3,
        },
        true,
        "native",
        Duration::from_millis(2),
    );
    assert!(!gate.enabled());
    s.handoff_update(
        &"b".repeat(32),
        super::super::handoff::Boundary {
            contacts: 0,
            neutral: true,
            revision: 4,
        },
        true,
        "native",
        Duration::from_millis(3),
    );
    let restore = s.handoff_take_restore().unwrap();
    s.handoff_prepared_restore(&restore, gate.remove_inhibition().is_ok());
    assert!(!gate.enabled());
    s.handoff_update(
        &"b".repeat(32),
        super::super::handoff::Boundary {
            contacts: 0,
            neutral: true,
            revision: 4,
        },
        true,
        "native",
        Duration::from_millis(3),
    );
    assert!(gate.enabled());
    assert_eq!(lease(&mut s, 3, "new")["error"], "retired");
    drop(s);
    drop(gate);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn model_native_queue_calls_the_real_vm_guard_and_reload_retires_it() {
    use std::os::{
        fd::AsFd,
        unix::{fs::DirBuilderExt, net::UnixStream},
    };
    let directory = std::path::PathBuf::from(format!("/tmp/mcg-{}", std::process::id()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let (connection, _) = UnixStream::pair().unwrap();
    let mut provider =
        super::super::Provider::bind_at(&directory, connection.as_fd(), "HEADLESS-1").unwrap();
    let mut runtime = morf_lua::Runtime::default();
    runtime
        .execute(
            "guard.lua",
            br#"
        local count=0
        morf.ipc["prek-native-control"]=function(payload,token)
            assert(__morf_prek_authorized(token))
            count=count+1
            return {status="accepted"}
        end
        morf.ipc.count=function() return count end
        morf.ipc["prek-native-handoff"]=function(token,enabled)
            assert(__morf_prek_handoff_authorized(token,enabled))
            return {ready=true,enabled=enabled,driver="native"}
        end
    "#,
        )
        .unwrap();
    assert!(!provider.dispatch(&mut runtime, 1));
    {
        let mut state = provider.controls.lock().unwrap();
        // Reuse the real queue and VM, with explicit modeled credentials.
        let at = now().unwrap().as_millis() as u64;
        let mut request = envelope("lease");
        request["epoch"] = json!(state.epoch);
        request["expires_ns"] = json!(((at + 1000) * 1_000_000).to_string());
        assert_eq!(
            state
                .request(
                    root_model(),
                    &request,
                    Duration::from_millis(at),
                    &"c".repeat(32)
                )
                .unwrap()["status"],
            "accepted"
        );
        // The helper has a fixed model epoch; use a temporary matching state
        let handoff = state.handoff.as_mut().unwrap();
        handoff
            .request(
                "handoff-quiesce",
                1,
                false,
                &"d".repeat(32),
                Duration::from_millis(at),
            )
            .unwrap();
        provider.gate.persist_inhibition().unwrap();
        handoff.update(
            super::super::handoff::Boundary {
                contacts: 0,
                neutral: true,
                revision: 1,
            },
            true,
            "native",
            Duration::from_millis(at),
        );
        handoff
            .request(
                "handoff-activate",
                1,
                false,
                "unused",
                Duration::from_millis(at),
            )
            .unwrap();
        // to produce an otherwise valid typed payload for the real queue.
        let mut fixture = super::tests::state();
        lease(&mut fixture, at, "fixture");
        submit(&mut fixture, 1, at);
        let mut request = envelope("submit");
        request["epoch"] = json!(state.epoch);
        request["id"] = json!("1");
        request["deadline_ns"] = json!(((at + 250) * 1_000_000).to_string());
        request["request"] = serde_json::from_str::<Value>(&fixture.pending[0].payload).unwrap();
        assert_eq!(
            state
                .request(root_model(), &request, Duration::from_millis(at), "unused")
                .unwrap()["status"],
            "queued"
        );
    }
    assert!(provider.dispatch(&mut runtime, 1));
    assert_eq!(runtime.call_ipc("count", &[]).unwrap()[0].to_json(), 1);
    assert_eq!(
        provider.controls.lock().unwrap().results[&1]["status"],
        "dispatched"
    );
    provider.dispatch(&mut runtime, 2);
    assert!(
        !provider
            .controls
            .lock()
            .unwrap()
            .valid(&"c".repeat(32), now().unwrap())
    );
    assert_eq!(runtime.call_ipc("count", &[]).unwrap()[0].to_json(), 1);
    drop(provider);
    std::fs::remove_dir_all(directory).unwrap();
}
