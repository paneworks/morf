// The primary runtime: exactly one runtime of the shell at a time does what
// must be done once, it stays put while other outputs come and go, and when
// its own output goes the duty moves -- after that runtime has ended.

use super::outputless::{Started, call, named, output, spawner};
use crate::config::LoadPolicy;
use crate::lock::{Worker, WorkerCommand, WorkerSender};
use crate::outputless::{OUTPUTLESS, Outputless, outputless_screen};
use crate::services::stop_workers;
use crate::workers::{
    Handover, Seed, WorkerStart, desired_workers, elect_primary, hand_over_primary,
    handle_worker_command, reconcile_with,
};
use morf_io::IpcValue as WireValue;
use morf_lua::{Limits, Runtime};
use morf_app::Output;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

const SHELL: &str = r#"
    morf.surface.outputless = true
    local heard = {}
    morf.on_primary(function(primary) heard[#heard + 1] = tostring(primary) end)
    local runs = 0
    morf.effect("follow primary", function() morf.primary() runs = runs + 1 end)
    morf.ipc.primary = function() return morf.primary() end
    morf.ipc.heard = function() return table.concat(heard, ",") end
    morf.ipc.runs = function() return runs end
    if #morf.screens > 0 then morf.ui.Item {} end
"#;

/// What one worker answers `verb` with.
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
    crate::services::wire_ipc_value(&values[0])
}

fn number(value: WireValue) -> i64 {
    match value {
        WireValue::Integer(value) => value,
        other => panic!("not an integer: {other:?}"),
    }
}

/// The workers that say they are primary, by name.
fn primaries(workers: &BTreeMap<String, Worker>) -> Vec<String> {
    workers
        .iter()
        .filter(|(_, worker)| ask(worker, "primary") == WireValue::Boolean(true))
        .map(|(name, _)| name.clone())
        .collect()
}

#[test]
fn the_first_output_is_primary_and_stays_while_others_come_and_go() {
    let a = output("A", 0);
    let b = output("B", 800);
    assert_eq!(
        elect_primary(None, &named(std::slice::from_ref(&a))),
        Some("A".into())
    );
    // Announced first wins, not the name: B's global is the lower one.
    let early_b = Output { id: 0, ..b.clone() };
    let late_a = Output {
        id: 900,
        ..a.clone()
    };
    assert_eq!(
        elect_primary(None, &named(&[late_a.clone(), early_b.clone()])),
        Some("B".into())
    );
    // Whoever is primary stays so while another output arrives or goes.
    assert_eq!(
        elect_primary(Some("A"), &named(&[late_a.clone(), early_b])),
        Some("A".into())
    );
    assert_eq!(
        elect_primary(Some("A"), &named(&[late_a])),
        Some("A".into())
    );
    // Its output gone, the next one takes it.
    assert_eq!(elect_primary(Some("A"), &named(&[b])), Some("B".into()));
    // With none, the outputless runtime, and with nothing, nobody.
    let dark = desired_workers(BTreeMap::new(), Outputless::Wanted);
    assert_eq!(elect_primary(Some("B"), &dark), Some(OUTPUTLESS.into()));
    assert_eq!(elect_primary(Some("B"), &BTreeMap::new()), None);
}

#[test]
fn exactly_one_runtime_is_primary_through_hotplug_and_going_dark() {
    let handover = Handover::default();
    let (tx, _rx) = mpsc::channel();
    let started = Started::default();
    let mut spawn = spawner(SHELL, &handover, &tx, &started);
    let mut workers = BTreeMap::new();
    let mut primary = None;
    let (a, b) = (output("A", 0), output("B", 800));

    // One output: it is primary, and knew it from its first line.
    reconcile_with(
        &mut workers,
        &named(std::slice::from_ref(&a)),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(primaries(&workers), ["A"]);
    assert_eq!(primary.as_deref(), Some("A"));
    assert_eq!(
        ask(&workers["A"], "heard"),
        WireValue::String(String::new())
    );

    // Two: still A, and B is not.
    reconcile_with(
        &mut workers,
        &named(&[a.clone(), b.clone()]),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(primaries(&workers), ["A"]);

    // Two to one, A's output gone: B takes it over, its effect runs again and
    // its callback hears it once.
    let runs = number(ask(&workers["B"], "runs"));
    reconcile_with(
        &mut workers,
        &named(std::slice::from_ref(&b)),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(primaries(&workers), ["B"]);
    assert_eq!(
        ask(&workers["B"], "heard"),
        WireValue::String("true".into())
    );
    assert_eq!(number(ask(&workers["B"], "runs")), runs + 1);

    // None at all: the outputless runtime.
    reconcile_with(
        &mut workers,
        &desired_workers(BTreeMap::new(), Outputless::Wanted),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(primaries(&workers), [OUTPUTLESS]);
    assert_eq!(workers[OUTPUTLESS].screen, outputless_screen());

    // The outputs come back: the first one announced.
    reconcile_with(
        &mut workers,
        &named(&[b.clone(), a.clone()]),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(primaries(&workers), ["A"]);

    // Reordered -- A moved right of B, its global now the later one: every
    // runtime is started again for its new geometry, and A keeps the duty.
    let moved_a = Output {
        id: 1600,
        position: Some((800, 0)),
        ..a.clone()
    };
    let moved_b = Output {
        position: Some((0, 0)),
        ..b.clone()
    };
    reconcile_with(
        &mut workers,
        &named(&[moved_b, moved_a]),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(primaries(&workers), ["A"]);
    assert_eq!(call(&workers, "primary").result.len(), 1);
    stop_workers(workers);
}

/// Workers whose stand-in loop writes down when it is told it is primary and
/// when its runtime has gone, into `events`.
fn logging_spawner<'a>(
    handover: &'a Handover,
    events: &'a Arc<Mutex<Vec<String>>>,
) -> impl FnMut(&str, &Output, Option<Seed>, bool) -> Worker + 'a {
    move |name, screen, seed, primary| {
        let stop = Arc::new(AtomicBool::new(false));
        let (commands, command_rx) = mpsc::channel();
        let (tx, _rx) = mpsc::channel();
        let start = WorkerStart {
            path: Arc::new(PathBuf::from("shell.lua")),
            source: Arc::from(SHELL.as_bytes()),
            policy: LoadPolicy::default(),
            tx,
            stop: Arc::clone(&stop),
            commands: command_rx,
            seed,
            handover: handover.clone(),
            primary,
        };
        let (name, events) = (name.to_owned(), Arc::clone(events));
        let screen_for_lua = crate::supervisor::lua_screen(screen);
        let join = thread::spawn(move || {
            let mut runtime = Runtime::for_screen(Limits::default(), screen_for_lua.clone());
            runtime.set_primary(start.primary);
            crate::supervisor::execute_config(
                &mut runtime,
                &start.path,
                &start.source,
                start.policy,
            )
            .unwrap();
            while !start.stop.load(Ordering::Acquire) {
                if let Ok(command) = start.commands.recv_timeout(Duration::from_millis(5)) {
                    if matches!(command, WorkerCommand::Primary(true)) {
                        events.lock().unwrap().push(format!("{name} told"));
                    }
                    handle_worker_command(
                        &mut runtime,
                        Some(&screen_for_lua),
                        start.policy,
                        command,
                    );
                }
            }
            drop(runtime);
            events.lock().unwrap().push(format!("{name} ended"));
        });
        Worker {
            stop,
            commands: WorkerSender::new(commands),
            join,
            screen: screen.clone(),
        }
    }
}

#[test]
fn the_duty_moves_only_after_the_runtime_that_held_it_has_ended() {
    let handover = Handover::default();
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut spawn = logging_spawner(&handover, &events);
    let mut workers = BTreeMap::new();
    let mut primary = None;
    let (a, b, c) = (output("A", 0), output("B", 800), output("C", 1600));
    reconcile_with(
        &mut workers,
        &named(&[a, b.clone(), c.clone()]),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(primaries(&workers), ["A"]);
    // Through a reconcile: A's output unplugged.
    reconcile_with(
        &mut workers,
        &named(&[b.clone(), c.clone()]),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(primaries(&workers), ["B"]);
    // Through a failure: B's surface closed under it, the supervisor lets it
    // go and hands over at once, before asking for the outputs again.
    let worker = workers.remove("B").unwrap();
    worker.request_stop();
    worker.join.join().unwrap();
    hand_over_primary(&workers, &mut primary);
    assert_eq!(primary.as_deref(), Some("C"));
    assert_eq!(primaries(&workers), ["C"]);
    stop_workers(workers);
    let events = events.lock().unwrap().clone();
    let at = |event: &str| {
        events
            .iter()
            .position(|seen| seen == event)
            .unwrap_or_else(|| panic!("no `{event}` in {events:?}"))
    };
    assert!(at("A ended") < at("B told"), "{events:?}");
    assert!(at("B ended") < at("C told"), "{events:?}");
    // Nobody else was ever told.
    assert_eq!(
        events
            .iter()
            .filter(|event| event.ends_with("told"))
            .count(),
        2,
        "{events:?}"
    );
}
