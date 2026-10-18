// A turned, resized or rescaled output keeps the runtime that drives it; a
// different output under the same name starts its own.

use super::outputless::{Started, named, output, spawner};
use morf_app::Output;
use morf_host::lock::{Worker, WorkerCommand};
use morf_host::process::services::stop_workers;
use morf_host::process::workers::{Handover, broadcast_screens, reconcile_with};
use morf_io::IpcValue as WireValue;
use std::collections::BTreeMap;
use std::sync::mpsc;
use std::time::Duration;

const SHELL: &str = r#"
    local loads = 0
    loads = loads + 1
    morf.ipc.loads = function() return loads end
    morf.ipc.size = function()
      local s = morf.screens[1] or {}
      return tostring(s.width) .. "x" .. tostring(s.height)
    end
    morf.ui.Item {}
"#;

fn ask(worker: &Worker, verb: &str) -> WireValue {
    let (reply, answer) = mpsc::sync_channel(1);
    worker
        .commands
        .send(WorkerCommand::Call {
            target: verb.to_owned(),
            args: Vec::new(),
            reply,
        })
        .unwrap();
    let values = answer
        .recv_timeout(Duration::from_secs(2))
        .expect("answered")
        .expect("not refused");
    morf_host::process::services::wire_ipc_value(&values[0])
}

fn text(value: WireValue) -> String {
    match value {
        WireValue::String(text) => text,
        other => panic!("not a string: {other:?}"),
    }
}

#[test]
fn a_rotated_output_keeps_its_worker_and_sees_its_new_size() {
    let handover = Handover::default();
    let (tx, _rx) = mpsc::channel();
    let started = Started::default();
    let mut spawn = spawner(SHELL, &handover, &tx, &started);
    let mut workers = BTreeMap::new();
    let mut primary = None;
    let upright = output("A", 0);

    reconcile_with(
        &mut workers,
        &named(std::slice::from_ref(&upright)),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(started.lock().unwrap().len(), 1);
    assert_eq!(text(ask(&workers["A"], "size")), "800x600");

    // Turned a quarter: the same output, laid out the other way round.
    let turned = Output {
        size: Some((600, 800)),
        pixels: Some((600, 800)),
        transform: "90",
        ..upright.clone()
    };
    broadcast_screens(&workers, std::slice::from_ref(&turned));
    reconcile_with(
        &mut workers,
        &named(std::slice::from_ref(&turned)),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(started.lock().unwrap().len(), 1, "no new worker for a turn");
    assert_eq!(workers["A"].screen, turned);
    assert_eq!(text(ask(&workers["A"], "size")), "600x800");

    // Rescaled: still the same output and the same worker.
    let rescaled = Output {
        scale: 2,
        ..turned.clone()
    };
    reconcile_with(
        &mut workers,
        &named(std::slice::from_ref(&rescaled)),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(
        started.lock().unwrap().len(),
        1,
        "no new worker for a rescale"
    );

    // Another global under the same name is another output: a new worker.
    let replugged = Output {
        id: upright.id + 50,
        ..rescaled
    };
    reconcile_with(
        &mut workers,
        &named(std::slice::from_ref(&replugged)),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(started.lock().unwrap().len(), 2, "a replug starts afresh");
    stop_workers(workers);
}
