// A lock process answers IPC on a socket of its own.

use crate::config::lock_variant;
use crate::lock_ipc::answer_lock_request;
use morf_io::{IpcRequest, IpcValue as WireValue};
use morf_lua::Runtime;
use std::path::Path;
use std::time::Instant;

#[test]
fn the_lock_socket_sits_beside_the_shells() {
    assert_eq!(
        lock_variant(Path::new("/run/user/1000/morf/wayland-1.sock")),
        Path::new("/run/user/1000/morf/wayland-1-lock.sock")
    );
}

#[test]
fn a_lock_answers_calls_and_refuses_to_be_killed() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "lock-ipc.lua",
            br##"
                local tries = 0
                morf.surface.session_lock = true
                morf.ipc.poke = function(word) tries = tries + 1 return word .. tries end
                require("morf.ui").Rect { color = "#000000" }
            "##,
        )
        .unwrap();
    let path = Path::new("lock.lua");
    let started = Instant::now();
    let (reply, changed) = answer_lock_request(
        &mut runtime,
        &IpcRequest::Call {
            target: "poke".into(),
            args: vec![WireValue::String("hi".into())],
        },
        path,
        started,
    );
    assert!(reply.ok && changed);
    assert_eq!(reply.result, [WireValue::String("hi1".into())]);

    let (verbs, _) = answer_lock_request(&mut runtime, &IpcRequest::Verbs, path, started);
    assert!(verbs.result.contains(&WireValue::String("poke".into())));

    let (info, _) = answer_lock_request(&mut runtime, &IpcRequest::Info, path, started);
    assert_eq!(
        info.result[0],
        WireValue::Integer(i64::from(std::process::id()))
    );

    let (kill, _) = answer_lock_request(&mut runtime, &IpcRequest::Kill, path, started);
    assert!(
        !kill.ok,
        "killing a lock client would leave the session locked"
    );
}
