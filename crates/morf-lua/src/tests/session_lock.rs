//! What the compositor says about the session lock reaches the configuration.

use super::*;

fn lock_runtime() -> Runtime {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "session-lock.lua",
            br#"
                local ui = require("morf.ui")
                local heard, confirmed = {}, 0
                morf.on_session_lock_state(function(state) heard[#heard + 1] = state end)
                morf.on_session_locked(function() confirmed = confirmed + 1 end)
                morf.ipc.heard = function() return table.concat(heard, ",") end
                morf.ipc.confirmed = function() return confirmed end
                morf.ipc.now = function() return morf.session_lock_state() end
                ui.Text { text = function() return morf.session_lock:get() end }
            "#,
        )
        .unwrap();
    runtime
}

fn call(runtime: &mut Runtime, verb: &str) -> IpcValue {
    runtime.call_ipc(verb, &[]).unwrap()[0].clone()
}

#[test]
fn a_confirmed_lock_is_heard_once_and_followed_by_bindings() {
    let mut runtime = lock_runtime();
    let root = runtime.scene().roots()[0];
    assert_eq!(runtime.session_lock_state(), SessionLockState::Unlocked);
    assert_eq!(
        call(&mut runtime, "now"),
        IpcValue::String("unlocked".into())
    );

    assert!(runtime.set_session_lock_state(SessionLockState::Pending));
    assert!(runtime.set_session_lock_state(SessionLockState::Locked));
    // The same answer twice is one change.
    assert!(!runtime.set_session_lock_state(SessionLockState::Locked));
    assert_eq!(
        runtime.scene().string_value(root, "text").unwrap(),
        "locked"
    );
    assert_eq!(call(&mut runtime, "now"), IpcValue::String("locked".into()));
    assert_eq!(call(&mut runtime, "confirmed"), IpcValue::Integer(1));

    assert!(runtime.set_session_lock_state(SessionLockState::Unlocked));
    assert_eq!(
        call(&mut runtime, "heard"),
        IpcValue::String("pending,locked,unlocked".into())
    );
    assert_eq!(call(&mut runtime, "confirmed"), IpcValue::Integer(1));
}

#[test]
fn a_refused_lock_is_failed_and_never_confirmed() {
    let mut runtime = lock_runtime();
    let root = runtime.scene().roots()[0];
    runtime.set_session_lock_state(SessionLockState::Pending);
    runtime.set_session_lock_state(SessionLockState::Failed);
    assert_eq!(
        runtime.scene().string_value(root, "text").unwrap(),
        "failed"
    );
    assert_eq!(
        call(&mut runtime, "heard"),
        IpcValue::String("pending,failed".into())
    );
    assert_eq!(call(&mut runtime, "confirmed"), IpcValue::Integer(0));
}
