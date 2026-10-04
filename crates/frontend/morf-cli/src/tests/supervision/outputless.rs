// The shell with no output: the supervisor keeps one runtime for the
// configuration while the compositor offers none, and hands over to the
// per-output ones when an output comes back.

use crate::config::LoadPolicy;
use morf_app::Output;
use morf_host::lock::{SupervisorMessage, Worker, WorkerCommand, WorkerMessage, WorkerSender};
use morf_host::outputless::{OUTPUTLESS, Outputless, outputless_screen, run_outputless};
use morf_host::services::stop_workers;
use morf_host::workers::{
    Handover, Seed, WorkerStart, desired_workers, handle_ipc, handle_worker_command, reconcile_with,
};
use morf_io::{IpcReply, IpcRequest, IpcValue as WireValue};
use morf_lua::{Limits, Runtime, Screen};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const SHELL: &str = r#"
    morf.surface.outputless = true
    local count = morf.reloadable("count", 0)
    morf.ipc.bump = function() count:set(count:get() + 1) return count:get() end
    morf.ipc.screens = function() return #morf.screens end
    morf.ipc.outputless = function() return morf.capabilities.outputless == true end
    local ticks = 0
    morf.timer(5, function() ticks = ticks + 1 end, true)
    morf.ipc.ticks = function() return ticks end
    if #morf.screens > 0 then morf.ui.Item {} end
"#;

pub(super) fn output(name: &str, x: i32) -> Output {
    Output {
        id: x as u32,
        name: Some(name.to_owned()),
        position: Some((x, 0)),
        size: Some((800, 600)),
        scale: 1,
        transform: "normal",
        ..Output::default()
    }
}

pub(super) fn named(outputs: &[Output]) -> BTreeMap<String, Output> {
    outputs
        .iter()
        .map(|screen| (screen.name.clone().unwrap(), screen.clone()))
        .collect()
}

/// What a test's workers were started with, in order.
pub(super) type Started = Arc<Mutex<Vec<(String, Option<Seed>)>>>;

/// Starts workers the way the supervisor does, with a stand-in for the
/// per-output loop (which needs a compositor): the same runtime, seeded and
/// handing over the same way, answering commands until it is stopped. The
/// outputless worker is the real one, without a Wayland connection.
pub(super) fn spawner<'a>(
    source: &'a str,
    handover: &'a Handover,
    tx: &'a mpsc::Sender<SupervisorMessage>,
    started: &'a Started,
) -> impl FnMut(&str, &Output, Option<Seed>, bool) -> Worker + 'a {
    move |name, screen, seed, primary| {
        started
            .lock()
            .unwrap()
            .push((name.to_owned(), seed.clone()));
        let stop = Arc::new(AtomicBool::new(false));
        let (commands, command_rx) = mpsc::channel();
        let start = WorkerStart {
            path: Arc::new(PathBuf::from("shell.lua")),
            source: Arc::from(source.as_bytes()),
            policy: LoadPolicy::default(),
            tx: tx.clone(),
            stop: Arc::clone(&stop),
            commands: command_rx,
            seed,
            handover: handover.clone(),
            primary,
        };
        let join = if name == OUTPUTLESS {
            thread::spawn(move || {
                run_outputless(start, false).unwrap();
            })
        } else {
            let screen = morf_host::supervisor::lua_screen(screen);
            thread::spawn(move || stand_in(start, screen))
        };
        Worker {
            stop,
            commands: WorkerSender::new(commands),
            join,
            screen: screen.clone(),
        }
    }
}

pub(super) fn stand_in(start: WorkerStart, screen: Screen) {
    let mut runtime = Runtime::for_screen(Limits::default(), screen.clone());
    if let Some(seed) = start.seed.clone() {
        runtime.restore_reloadable_state(seed);
    }
    runtime.set_primary(start.primary);
    morf_host::supervisor::execute_config(&mut runtime, &start.path, &start.source, start.policy)
        .unwrap();
    while !start.stop.load(Ordering::Acquire) {
        if let Ok(command) = start.commands.recv_timeout(Duration::from_millis(5)) {
            handle_worker_command(&mut runtime, Some(&screen), start.policy, command);
        }
    }
    start.handover.deposit(&mut runtime);
}

pub(super) fn call(workers: &BTreeMap<String, Worker>, verb: &str) -> IpcReply {
    handle_ipc(
        workers,
        &mut Vec::new(),
        &IpcRequest::Call {
            target: verb.to_owned(),
            args: Vec::new(),
        },
    )
}

pub(super) fn integer(reply: IpcReply) -> i64 {
    assert!(reply.ok, "refused: {:?}", reply.error);
    match reply.result.first() {
        Some(WireValue::Integer(value)) => *value,
        other => panic!("not an integer: {other:?}"),
    }
}

/// Keeps asking until `verb` answers at least `least`.
fn wait_for(workers: &BTreeMap<String, Worker>, verb: &str, least: i64) -> i64 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let reply = call(workers, verb);
        if let Some(WireValue::Integer(value)) = reply.result.first()
            && (*value >= least || Instant::now() > deadline)
        {
            return *value;
        }
        assert!(Instant::now() < deadline, "{verb} never answered");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn with_no_output_the_supervisor_wants_the_outputless_runtime() {
    let one = named(&[output("A", 0)]);
    assert_eq!(desired_workers(one.clone(), Outputless::Unknown), one);
    assert_eq!(desired_workers(one.clone(), Outputless::Unwanted), one);
    // None at all: run to find out, run because asked, or nothing.
    for wanted in [Outputless::Unknown, Outputless::Wanted] {
        let desired = desired_workers(BTreeMap::new(), wanted);
        assert_eq!(desired.keys().collect::<Vec<_>>(), [OUTPUTLESS]);
        assert_eq!(desired[OUTPUTLESS], outputless_screen());
    }
    assert!(desired_workers(BTreeMap::new(), Outputless::Unwanted).is_empty());
}

fn round_trip(outputs: &[Output]) {
    let handover = Handover::default();
    let (tx, _rx) = mpsc::channel();
    let started = Started::default();
    let mut spawn = spawner(SHELL, &handover, &tx, &started);
    let mut workers = BTreeMap::new();
    let mut primary = None;
    let lit = named(outputs);

    reconcile_with(&mut workers, &lit, &handover, &mut primary, &mut spawn);
    assert_eq!(workers.len(), outputs.len());
    assert_eq!(integer(call(&workers, "bump")), 1);

    // Every output gone: one runtime, for none of them, starting from what
    // the last output's kept.
    let dark = desired_workers(BTreeMap::new(), Outputless::Wanted);
    reconcile_with(&mut workers, &dark, &handover, &mut primary, &mut spawn);
    assert_eq!(workers.keys().collect::<Vec<_>>(), [OUTPUTLESS]);
    // IPC reaches it, `morf.screens` is empty, the capability says so, and
    // its timers run.
    assert_eq!(integer(call(&workers, "screens")), 0);
    assert_eq!(
        call(&workers, "outputless"),
        IpcReply::success(vec![WireValue::Boolean(true)])
    );
    assert!(wait_for(&workers, "ticks", 3) >= 3);
    assert_eq!(integer(call(&workers, "bump")), 2);
    // The same list again changes nothing.
    reconcile_with(&mut workers, &dark, &handover, &mut primary, &mut spawn);
    assert_eq!(started.lock().unwrap().len(), outputs.len() + 1);

    // The outputs come back: the outputless runtime goes, and every output's
    // starts from what it kept.
    reconcile_with(&mut workers, &lit, &handover, &mut primary, &mut spawn);
    assert_eq!(workers.len(), outputs.len());
    assert!(!workers.contains_key(OUTPUTLESS));
    assert!(integer(call(&workers, "screens")) >= 1);
    assert_eq!(integer(call(&workers, "bump")), 3);
    stop_workers(workers);

    let started = started.lock().unwrap();
    let count = |seed: &Option<Seed>| match seed.as_ref().and_then(|seed| seed.get("count")) {
        Some(morf_value::IpcValue::Integer(value)) => Some(*value),
        _ => None,
    };
    // First the outputs, fresh; the outputless runtime from 1; the outputs
    // again from 2.
    let seeds = started
        .iter()
        .map(|(name, seed)| (name.as_str(), count(seed)))
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    expected.extend(outputs.iter().map(|o| (o.name.as_deref().unwrap(), None)));
    expected.push((OUTPUTLESS, Some(1)));
    expected.extend(
        outputs
            .iter()
            .map(|o| (o.name.as_deref().unwrap(), Some(2))),
    );
    assert_eq!(seeds, expected);
}

#[test]
fn one_output_goes_dark_and_comes_back() {
    round_trip(&[output("A", 0)]);
}

#[test]
fn two_outputs_go_dark_and_come_back() {
    round_trip(&[output("A", 0), output("B", 800)]);
}

#[test]
fn a_hotplug_beside_a_lit_output_hands_nothing_over() {
    let handover = Handover::default();
    let (tx, _rx) = mpsc::channel();
    let started = Started::default();
    let mut spawn = spawner(SHELL, &handover, &tx, &started);
    let mut workers = BTreeMap::new();
    let mut primary = None;
    reconcile_with(
        &mut workers,
        &named(&[output("A", 0)]),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(integer(call(&workers, "bump")), 1);
    // B arrives while A stays: B starts fresh, as it always has.
    reconcile_with(
        &mut workers,
        &named(&[output("A", 0), output("B", 800)]),
        &handover,
        &mut primary,
        &mut spawn,
    );
    // B leaves again: A is untouched and nothing is kept for later.
    reconcile_with(
        &mut workers,
        &named(&[output("A", 0)]),
        &handover,
        &mut primary,
        &mut spawn,
    );
    stop_workers(workers);
    let started = started.lock().unwrap();
    assert_eq!(started.len(), 2);
    assert!(started.iter().all(|(_, seed)| seed.is_none()));
}

#[test]
fn with_every_output_gone_a_shell_that_did_not_ask_waits() {
    let handover = Handover::default();
    let (tx, rx) = mpsc::channel();
    let started = Started::default();
    // Draws only: it never said it runs with no output.
    let mut spawn = spawner("morf.ui.Item {}", &handover, &tx, &started);
    let mut workers = BTreeMap::new();
    let mut primary = None;
    reconcile_with(
        &mut workers,
        &desired_workers(BTreeMap::new(), Outputless::Unknown),
        &handover,
        &mut primary,
        &mut spawn,
    );
    // Run once to hear it; it says no and ends by itself.
    let worker = workers.remove(OUTPUTLESS).unwrap();
    worker.join.join().unwrap();
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(1)),
        Ok(SupervisorMessage::Worker(WorkerMessage::Loaded {
            outputless: false,
            session_lock: false,
            ..
        }))
    ));
    // And nothing is started for it afterwards; IPC says there is no one.
    reconcile_with(
        &mut workers,
        &desired_workers(BTreeMap::new(), Outputless::Unwanted),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert!(workers.is_empty());
    assert_eq!(
        call(&workers, "anything"),
        IpcReply::refused("shell has no active output")
    );
}

#[test]
fn a_configuration_that_needs_a_screen_is_not_a_failed_shell() {
    let handover = Handover::default();
    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let (_commands, command_rx) = mpsc::channel::<WorkerCommand>();
    // Written for a screen: with none it fails to load.
    let start = WorkerStart {
        path: Arc::new(PathBuf::from("shell.lua")),
        source: Arc::from(&b"local width = morf.screens[1].width"[..]),
        policy: LoadPolicy::default(),
        tx,
        stop,
        commands: command_rx,
        seed: None,
        handover,
        primary: true,
    };
    assert_eq!(run_outputless(start, false), Ok(()));
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(1)),
        Ok(SupervisorMessage::Worker(WorkerMessage::Loaded {
            outputless: false,
            ..
        }))
    ));
}

#[test]
fn a_reload_while_outputless_says_whether_it_still_wants_to_be() {
    let handover = Handover::default();
    let (tx, rx) = mpsc::channel();
    let started = Started::default();
    let mut spawn = spawner(SHELL, &handover, &tx, &started);
    let mut workers = BTreeMap::new();
    let mut primary = None;
    reconcile_with(
        &mut workers,
        &desired_workers(BTreeMap::new(), Outputless::Wanted),
        &handover,
        &mut primary,
        &mut spawn,
    );
    let (reply, result) = mpsc::sync_channel(1);
    workers[OUTPUTLESS]
        .commands
        .send(WorkerCommand::Reload {
            path: Arc::new(PathBuf::from("shell.lua")),
            source: Arc::from(&b"morf.surface.outputless = false"[..]),
            hard: false,
            reply,
        })
        .unwrap();
    assert_eq!(result.recv_timeout(Duration::from_secs(2)).unwrap(), Ok(()));
    let said = rx
        .iter()
        .find_map(|message| match message {
            SupervisorMessage::Worker(WorkerMessage::Outputless { output, wanted }) => {
                Some((output, wanted))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(said, (OUTPUTLESS.to_owned(), false));
    stop_workers(workers);
}

#[test]
fn going_dark_hands_over_what_the_primary_output_kept() {
    // Each output keeps its own values; only the primary one's differ here.
    // B, not primary, stops after A in name order, and used to leave its
    // own for the outputless runtime.
    let source = format!(
        "{SHELL}\n morf.ipc.mine = function() if morf.primary() then count:set(10) end end"
    );
    let handover = Handover::default();
    let (tx, _rx) = mpsc::channel();
    let started = Started::default();
    let mut spawn = spawner(&source, &handover, &tx, &started);
    let mut workers = BTreeMap::new();
    let mut primary = None;
    reconcile_with(
        &mut workers,
        &named(&[output("A", 0), output("B", 800)]),
        &handover,
        &mut primary,
        &mut spawn,
    );
    assert_eq!(primary.as_deref(), Some("A"));
    for worker in workers.values() {
        let (reply, answer) = mpsc::sync_channel(1);
        worker
            .commands
            .send(WorkerCommand::Call {
                target: "mine".to_owned(),
                args: Vec::new(),
                reply,
            })
            .unwrap();
        answer
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
    }
    let dark = desired_workers(BTreeMap::new(), Outputless::Wanted);
    reconcile_with(&mut workers, &dark, &handover, &mut primary, &mut spawn);
    assert_eq!(integer(call(&workers, "bump")), 11);
    stop_workers(workers);
}
