mod commands;

use morf_app::Output;
use morf_lua::{LogEntry, Runtime};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self};

use crate::{lock::*, outputless::*, supervisor::*, surface_run::*};

pub use commands::{WorkerUpdate, handle_ipc, handle_worker_command};

/// Values a runtime marked `morf.reloadable`, carried to its replacement.
pub type Seed = BTreeMap<String, morf_value::IpcValue>;

/// What a worker leaves behind when it ends.
///
/// Its reloadable values, for the runtimes that take over from it across the
/// line between "some outputs" and "none": the outputless runtime starts from
/// what the last output's had, and the outputs that come back start from
/// what it had. And its log lines not yet read, which `morf log` shows with
/// the supervisor's own: an output that went away took them with it before.
#[derive(Clone, Default)]
pub struct Handover(Arc<Mutex<Left>>);

#[derive(Default)]
struct Left {
    seed: Option<Seed>,
    logs: Vec<String>,
}

impl Handover {
    fn left(&self) -> std::sync::MutexGuard<'_, Left> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Keeps a finished runtime's values and unread log lines; one with no
    /// values leaves what an earlier one kept.
    pub fn deposit(&self, runtime: &mut Runtime) {
        let seed = runtime.reloadable_state();
        let logs = runtime.take_logs();
        let mut left = self.left();
        if !seed.is_empty() {
            left.seed = Some(seed);
        }
        left.logs.extend(logs.iter().map(LogEntry::to_wire));
    }

    pub fn take(&self) -> Option<Seed> {
        self.left().seed.take()
    }

    /// The log lines finished workers left, in the order they ended.
    pub fn take_logs(&self) -> Vec<String> {
        std::mem::take(&mut self.left().logs)
    }
}

/// Everything a worker thread is started with.
pub struct WorkerStart {
    pub path: Arc<PathBuf>,
    pub source: Arc<[u8]>,
    pub policy: LoadPolicy,
    pub tx: mpsc::Sender<SupervisorMessage>,
    pub stop: Arc<AtomicBool>,
    pub commands: mpsc::Receiver<WorkerCommand>,
    /// Reloadable values to start from, across an outputless handover.
    pub seed: Option<Seed>,
    /// Where this worker leaves its own when it ends.
    pub handover: Handover,
    /// Whether its runtime starts as the primary one (`morf.primary()`).
    pub primary: bool,
}

/// The workers the supervisor wants for an output list: one per named
/// output, or -- with none, unless the configuration said it does not want
/// it -- the one outputless runtime, or nothing at all.
pub fn desired_workers(
    named: BTreeMap<String, Output>,
    outputless: Outputless,
) -> BTreeMap<String, Output> {
    if !named.is_empty() || outputless == Outputless::Unwanted {
        return named;
    }
    BTreeMap::from([(OUTPUTLESS.to_owned(), outputless_screen())])
}

/// What every worker the supervisor starts shares.
pub struct WorkerContext<'a> {
    pub path: &'a Arc<PathBuf>,
    pub source: &'a Arc<[u8]>,
    pub policy: LoadPolicy,
    pub tx: &'a mpsc::Sender<SupervisorMessage>,
    pub handover: &'a Handover,
}

pub fn reconcile_workers(
    workers: &mut BTreeMap<String, Worker>,
    desired: &BTreeMap<String, Output>,
    primary: &mut Option<String>,
    context: &WorkerContext<'_>,
) {
    // An application (`morf app`) is one runtime, whatever the outputs: the
    // primary's, its windows opened wherever the compositor puts them.
    let only: BTreeMap<String, Output>;
    let desired = if crate::app::is_app() {
        let chosen = elect_primary(primary.as_deref(), desired);
        only = desired
            .iter()
            .filter(|(name, _)| Some(*name) == chosen.as_ref())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        &only
    } else {
        desired
    };
    reconcile_with(
        workers,
        desired,
        context.handover,
        primary,
        |name, screen, seed, is_primary| {
            let (commands, command_rx) = mpsc::channel();
            let stop = Arc::new(AtomicBool::new(false));
            let join = spawn_worker(
                name,
                screen,
                WorkerStart {
                    path: Arc::clone(context.path),
                    source: Arc::clone(context.source),
                    policy: context.policy,
                    tx: context.tx.clone(),
                    stop: Arc::clone(&stop),
                    commands: command_rx,
                    seed,
                    handover: context.handover.clone(),
                    primary: is_primary,
                },
            );
            Worker {
                stop,
                commands: WorkerSender::new(commands),
                join,
                screen: screen.clone(),
            }
        },
    );
}

/// Which runtime is the primary one (`morf.primary()`), given the one that
/// is now and the runtimes there are to be.
///
/// Exactly one while there is any runtime at all: the one that is stays, so
/// the duty never moves because another output came, went or moved; when
/// its runtime is gone (its output unplugged), the output the compositor
/// announced first takes it -- the lowest `wl_output` global, ties by name
/// -- and with no output the outputless runtime, the only one there is.
pub fn elect_primary(current: Option<&str>, desired: &BTreeMap<String, Output>) -> Option<String> {
    if let Some(current) = current
        && desired.contains_key(current)
    {
        return Some(current.to_owned());
    }
    desired
        .iter()
        .min_by(|(a, first), (b, second)| first.id.cmp(&second.id).then_with(|| a.cmp(b)))
        .map(|(name, _)| name.clone())
}

/// Moves the primary duty after the runtime holding it has ended, to one of
/// `workers` -- already running -- and tells that one. The runtime that held
/// it has been joined, so its bus names are free (`Runtime::drop` gives them
/// back) before the next one hears it is primary.
pub fn hand_over_primary(workers: &BTreeMap<String, Worker>, primary: &mut Option<String>) {
    let live = workers
        .iter()
        .map(|(name, worker)| (name.clone(), worker.screen.clone()))
        .collect::<BTreeMap<_, _>>();
    let next = elect_primary(primary.as_deref(), &live);
    if next != *primary
        && let Some(name) = &next
        && let Some(worker) = workers.get(name)
    {
        let _ = worker.commands.send(WorkerCommand::Primary(true));
    }
    *primary = next;
}

/// Brings `workers` to `desired`: stops the ones whose output went or
/// changed, then starts the missing ones with `spawn`. Crossing between the
/// outputless runtime and per-output ones, the runtimes that start are handed
/// the reloadable values the ones that stopped left behind.
///
/// `primary` names the primary runtime ([`elect_primary`]). It is decided
/// once the stale runtimes have stopped: a runtime already running that
/// takes it over is told so (`WorkerCommand::Primary`), and one started here
/// starts with it (`spawn`'s last argument), so every runtime's
/// configuration reads the right answer from its first line.
pub fn reconcile_with(
    workers: &mut BTreeMap<String, Worker>,
    desired: &BTreeMap<String, Output>,
    handover: &Handover,
    primary: &mut Option<String>,
    mut spawn: impl FnMut(&str, &Output, Option<Seed>, bool) -> Worker,
) {
    let was_outputless = workers.contains_key(OUTPUTLESS);
    let crossing = workers.is_empty() || was_outputless != desired.contains_key(OUTPUTLESS);
    let mut stale = workers
        .iter()
        .filter(|(name, worker)| desired.get(*name) != Some(&worker.screen))
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    // The primary goes last, so its values are the ones handed over: every
    // output keeps its own, and one still behind on a verb the others heard
    // would otherwise leave older ones for whatever starts next.
    stale.sort_by_key(|name| primary.as_deref() == Some(name.as_str()));
    for name in stale {
        let worker = workers.remove(&name).expect("worker key is present");
        worker.request_stop();
        let _ = worker.join.join();
    }
    // Every runtime that is going has gone -- and given its bus names back
    // -- before any is told it is primary.
    let next = elect_primary(primary.as_deref(), desired);
    let was_primary = |name: &str| primary.as_deref() == Some(name);
    if let Some(name) = &next
        && !was_primary(name)
        && let Some(worker) = workers.get(name)
    {
        let _ = worker.commands.send(WorkerCommand::Primary(true));
    }
    *primary = next;
    // Nothing starts: whatever was left is kept for whatever does.
    if desired.keys().all(|name| workers.contains_key(name)) {
        return;
    }
    let seed = handover.take().filter(|_| crossing);
    for (name, screen) in desired {
        if workers.contains_key(name) {
            continue;
        }
        let is_primary = primary.as_deref() == Some(name.as_str());
        let worker = spawn(name, screen, seed.clone(), is_primary);
        workers.insert(name.clone(), worker);
    }
}

/// Starts the thread that runs the configuration for one output, or for none.
fn spawn_worker(name: &str, screen: &Output, start: WorkerStart) -> thread::JoinHandle<()> {
    let output = name.to_owned();
    let screen = screen.clone();
    // Named after the output it drives, so anything reporting per-thread —
    // a profiler, a panic, a frame counter — says which screen it means.
    thread::Builder::new()
        .name(output.clone())
        .spawn(move || {
            let tx = start.tx.clone();
            let stop = Arc::clone(&start.stop);
            let result = if output == OUTPUTLESS {
                run_outputless(start, true)
            } else {
                run_surface(start, screen)
            };
            if let Err(error) = result
                && !stop.load(Ordering::Acquire)
            {
                let _ = tx.send(SupervisorMessage::Worker(WorkerMessage::Failed {
                    output,
                    error,
                }));
            }
        })
        .expect("worker thread")
}

/// Hands every live worker the compositor's new output list, so each runtime's
/// `morf.screens` follows a monitor being plugged in, moved, or unplugged.
pub fn broadcast_screens(workers: &BTreeMap<String, Worker>, screens: &[Output]) {
    for worker in workers.values() {
        let _ = worker
            .commands
            .send(WorkerCommand::Screens(screens.to_vec()));
    }
}
