// A lock process answers IPC on a socket of its own.

use morf_host::lock::ipc::answer_lock_request;
use morf_host::process::socket_path::lock_variant;
use morf_io::{IpcRequest, IpcValue as WireValue};
use morf_lua::Runtime;
use std::path::Path;
use std::time::Instant;

#[test]
fn native_lock_provider_sockets_are_not_public_shell_instances() {
    use std::{fs, os::unix::net::UnixListener};
    let directory = std::env::temp_dir().join(format!(
        "morf-native-list-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let names = [
        "wayland-1.sock",
        "wayland-1-lock.sock",
        "123-aabb-lock-provider.sock",
        "123-aabb-shell-provider.sock",
    ];
    let listeners: Vec<_> = names
        .iter()
        .map(|name| UnixListener::bind(directory.join(name)).unwrap())
        .collect();
    let mut listed = crate::commands::sockets_in(&directory).unwrap();
    listed.sort();
    let mut expected = vec![directory.join(names[0]), directory.join(names[1])];
    expected.sort();
    assert_eq!(listed, expected);
    drop(listeners);
    fs::remove_dir_all(directory).unwrap();
}

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
    assert!(reply.ok && !changed, "nonvisual state does not owe a frame");
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

#[test]
fn a_held_lock_refuses_every_verb_but_the_allowed_ones() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "lock-held.lua",
            br##"
                morf.surface.session_lock = true
                morf.ipc.unlock = function() morf.surface.session_lock = false return "open" end
                morf.ipc.status = function() return "locked" end
                require("morf.ui").Rect { color = "#000000" }
            "##,
        )
        .unwrap();
    runtime.set_session_lock_state(morf_lua::SessionLockState::Locked);
    let path = Path::new("lock.lua");
    let started = Instant::now();
    let call = |target: &str| IpcRequest::Call {
        target: target.into(),
        args: Vec::new(),
    };
    let (unlock, changed) = answer_lock_request(&mut runtime, &call("unlock"), path, started);
    assert!(!unlock.ok && !changed, "{unlock:?}");
    assert!(runtime.layer_surface_config().session_lock);
    let (status, _) = answer_lock_request(&mut runtime, &call("status"), path, started);
    assert_eq!(status.result, [WireValue::String("locked".into())]);
}

#[test]
fn lock_queries_do_not_repaint_and_partial_visual_errors_still_do() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "lock-paint.lua",
            br##"
        local node=require("morf.ui").Rect {width=10,height=10}
        morf.ipc.status=function() return "locked" end
        morf.ipc.keyboard_show=function() node.width=20 end
        morf.ipc.keyboard_hide=function() node.width=30 error("partial") end
    "##,
        )
        .unwrap();
    runtime.set_session_lock_state(morf_lua::SessionLockState::Locked);
    for (verb, repaint, success) in [
        ("status", false, true),
        ("keyboard_show", true, true),
        ("keyboard_hide", true, false),
        ("unlock", false, false),
    ] {
        let (reply, changed) = answer_lock_request(
            &mut runtime,
            &IpcRequest::Call {
                target: verb.into(),
                args: vec![],
            },
            Path::new("lock.lua"),
            Instant::now(),
        );
        assert_eq!(changed, repaint, "{verb}");
        assert_eq!(reply.ok, success, "{verb}");
    }
}
