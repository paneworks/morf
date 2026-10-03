mod frame;
mod layers;
mod layout_cache;
mod outputless;
mod pacing;
mod popups;
mod primary;
mod screens;

// The supervisor and its workers: screen sets, IPC dispatch, hot reload.

use crate::lock::Worker;
use crate::lock::WorkerCommand;
use crate::services::stop_workers;
use crate::supervisor::runtime_scripts;
use crate::workers::handle_ipc;
use crate::workers::handle_worker_command;
use morf_io::IpcReply;
use morf_io::IpcRequest;
use morf_io::IpcValue as WireValue;
use morf_lua::IpcValue;
use morf_lua::{Limits, Runtime, Screen};
use morf_wayland::ScreenInfo;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use crate::*;

#[test]
fn plugin_path_preserves_root_order() {
    let base = std::env::temp_dir().join(format!("morf-plugin-order-{}", std::process::id()));
    let first = base.join("z-first/plugin");
    let second = base.join("a-second/plugin");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    fs::write(first.join("entry.lua"), b"").unwrap();
    fs::write(second.join("entry.lua"), b"").unwrap();

    let scripts = runtime_scripts(&[base.join("z-first"), base.join("a-second")], "plugin");

    assert!(scripts[0].starts_with(base.join("z-first")));
    assert!(scripts[1].starts_with(base.join("a-second")));
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn successful_reload_carries_opt_in_state() {
    let screen = Screen {
        name: "test".into(),
        width: None,
        height: None,
        scale: 1,
        ..Screen::default()
    };
    let source = br#"
            local value = morf.reloadable("counter", 0)
            local completed = false
            morf.on_reload_completed(function() completed = true end)
            morf.ipc["counter.set"] = function(next) value:set(next) end
            morf.ipc["counter.get"] = function() return value:get() end
            morf.ipc["reload.completed"] = function() return completed end
            morf.ui.Item {}
        "#;
    let mut runtime = Runtime::for_screen(Limits::default(), screen.clone());
    runtime.execute("shell.lua", source).unwrap();
    runtime
        .call_ipc("counter.set", &[IpcValue::Integer(7)])
        .unwrap();
    let (reply, result) = mpsc::sync_channel(1);

    let update = handle_worker_command(
        &mut runtime,
        Some(&screen),
        LoadPolicy::default(),
        WorkerCommand::Reload {
            path: Arc::new(PathBuf::from("shell.lua")),
            source: Arc::from(&source[..]),
            hard: false,
            reply,
        },
    );

    assert!(result.recv().unwrap().is_ok());
    assert!(update.repaint);
    assert!(!update.recreate_surface);
    assert_eq!(
        runtime.call_ipc("counter.get", &[]).unwrap(),
        [IpcValue::Integer(7)]
    );
    assert_eq!(
        runtime.call_ipc("reload.completed", &[]).unwrap(),
        [IpcValue::Boolean(true)]
    );
}

#[test]
fn hard_reload_discards_opt_in_state() {
    let screen = Screen {
        name: "test".into(),
        width: None,
        height: None,
        scale: 1,
        ..Screen::default()
    };
    let source = br#"
            local value = morf.reloadable("counter", 0)
            morf.ipc["counter.set"] = function(next) value:set(next) end
            morf.ipc["counter.get"] = function() return value:get() end
            morf.ui.Item {}
        "#;
    let mut runtime = Runtime::for_screen(Limits::default(), screen.clone());
    runtime.execute("shell.lua", source).unwrap();
    runtime
        .call_ipc("counter.set", &[IpcValue::Integer(7)])
        .unwrap();
    let (reply, result) = mpsc::sync_channel(1);

    let update = handle_worker_command(
        &mut runtime,
        Some(&screen),
        LoadPolicy::default(),
        WorkerCommand::Reload {
            path: Arc::new(PathBuf::from("shell.lua")),
            source: Arc::from(&source[..]),
            hard: true,
            reply,
        },
    );

    assert!(result.recv().unwrap().is_ok());
    assert!(update.repaint);
    assert!(update.recreate_surface);
    assert_eq!(
        runtime.call_ipc("counter.get", &[]).unwrap(),
        [IpcValue::Integer(0)]
    );
}

#[test]
fn failed_reload_keeps_the_previous_runtime() {
    let screen = Screen {
        name: "test".into(),
        width: None,
        height: None,
        scale: 1,
        ..Screen::default()
    };
    let mut runtime = Runtime::for_screen(Limits::default(), screen.clone());
    runtime
        .execute(
            "shell.lua",
            br#"
                    local failure = ""
                    morf.on_reload_failed(function(error) failure = error end)
                    morf.ipc.value = function() return 7 end
                    morf.ipc["reload.failure"] = function() return failure end
                    morf.ui.Item {}
                "#,
        )
        .unwrap();
    let (reply, result) = mpsc::sync_channel(1);

    let update = handle_worker_command(
        &mut runtime,
        Some(&screen),
        LoadPolicy::default(),
        WorkerCommand::Reload {
            path: Arc::new(PathBuf::from("shell.lua")),
            source: Arc::from(&b"local ="[..]),
            hard: false,
            reply,
        },
    );

    assert!(result.recv().unwrap().is_err());
    assert!(!update.repaint);
    assert!(!update.recreate_surface);
    assert_eq!(
        runtime.call_ipc("value", &[]).unwrap(),
        [IpcValue::Integer(7)]
    );
    let failure = runtime.call_ipc("reload.failure", &[]).unwrap();
    assert!(matches!(&failure[..], [IpcValue::String(error)] if !error.is_empty()));
}

#[test]
fn supervisor_dispatches_registered_ipc_handler() {
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let (commands, rx) = mpsc::channel();
    let join = thread::spawn(move || {
        let mut runtime = Runtime::default();
        runtime
            .execute(
                "ipc.lua",
                b"morf.ipc.echo = function(value) return value, 2 end",
            )
            .unwrap();
        while !worker_stop.load(Ordering::Acquire) {
            if let Ok(command) = rx.recv_timeout(Duration::from_millis(10)) {
                handle_worker_command(
                    &mut runtime,
                    Some(&Screen {
                        name: "test".into(),
                        width: None,
                        height: None,
                        scale: 1,
                        ..Screen::default()
                    }),
                    LoadPolicy::default(),
                    command,
                );
            }
        }
    });
    let workers = BTreeMap::from([(
        "test".to_owned(),
        Worker {
            stop,
            commands: crate::lock::WorkerSender::new(commands),
            join,
            screen: ScreenInfo {
                name: Some("test".to_owned()),
                ..ScreenInfo::default()
            },
        },
    )]);

    let reply = handle_ipc(
        &workers,
        &mut Vec::new(),
        &IpcRequest::Call {
            target: "echo".into(),
            args: vec![WireValue::String("ready".into())],
        },
    );
    assert_eq!(
        reply,
        IpcReply::success(vec![
            WireValue::String("ready".into()),
            WireValue::Integer(2)
        ])
    );

    let (reload, result) = mpsc::sync_channel(1);
    workers["test"]
        .commands
        .send(WorkerCommand::Reload {
            path: Arc::new(PathBuf::from("shell.lua")),
            source: Arc::from(&b"this is not lua"[..]),
            hard: false,
            reply: reload,
        })
        .unwrap();
    assert!(
        result
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .is_err()
    );
    let reply = handle_ipc(
        &workers,
        &mut Vec::new(),
        &IpcRequest::Call {
            target: "echo".into(),
            args: vec![WireValue::String("last-good".into())],
        },
    );
    assert_eq!(
        reply,
        IpcReply::success(vec![
            WireValue::String("last-good".into()),
            WireValue::Integer(2)
        ])
    );
    stop_workers(workers);
}

// The workers run the configuration and say what it asked to be; there is
// no run before them. A shell goes on, a second shell on a taken display
// stops, and a lock is a lock whether or not a shell holds the socket.
#[test]
fn a_loaded_configuration_decides_what_the_supervisor_does() {
    use crate::supervisor::{LoadedStep, loaded_step};
    assert_eq!(loaded_step(false, false), LoadedStep::Run);
    assert_eq!(loaded_step(false, true), LoadedStep::Refuse);
    assert_eq!(loaded_step(true, false), LoadedStep::Lock);
    assert_eq!(loaded_step(true, true), LoadedStep::Lock);
}

#[test]
fn an_output_going_away_does_not_stop_the_shell() {
    use crate::supervisor::{FailureStep, SURFACE_CLOSED, failure_step};
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};
    let mut closures = VecDeque::new();
    let start = Instant::now();
    // A screen switched off closes its surfaces: the shell asks again.
    assert_eq!(
        failure_step(SURFACE_CLOSED, &mut closures, start),
        FailureStep::Probe
    );
    // Anything else is the shell's own failure.
    assert_eq!(
        failure_step("GPU lost", &mut closures, start),
        FailureStep::Stop
    );
    // A compositor closing every surface it is given is not asked forever.
    for _ in 0..4 {
        assert_eq!(
            failure_step(SURFACE_CLOSED, &mut closures, start),
            FailureStep::Probe
        );
    }
    assert_eq!(
        failure_step(SURFACE_CLOSED, &mut closures, start),
        FailureStep::Stop
    );
    // A minute on, it is news again.
    let later = start + Duration::from_secs(61);
    let mut closures = VecDeque::from(vec![start; 5]);
    assert_eq!(
        failure_step(SURFACE_CLOSED, &mut closures, later),
        FailureStep::Probe
    );
    assert_eq!(closures.len(), 1);
}
