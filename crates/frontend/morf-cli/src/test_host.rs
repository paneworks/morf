//! The host half of `morf.test`: the functions a spec calls, each acting on
//! the configuration under test.
//!
//! A spec's runtime and the configuration's are two runtimes. The spec calls
//! `__morf_test.click(...)`, which lands here, which drives the other one
//! through [`Headless`] -- the same code the shell's own loop runs.

mod input;
mod pixels;
mod query;
mod subject;
mod values;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use morf_lua::{LogLevel, Runtime};
use morf_scene::NodeHandle;
use morf_value::{IpcTable, IpcValue};

use crate::runner_args::RunnerArgs;
use input::{click, key, leave, motion, press, touch, type_text, wheel};
use morf_host::headless::Headless;
use query::{accessible, accessible_action, nodes, text_of};
use subject::{
    approve, frames, ipc, load, logs, reload, resize_window, snapshot, stub_run, surfaces,
};
use values::{number, optional_text};

/// What a spec file shares between its tests.
pub(crate) struct TestHost {
    pub(crate) spec_dir: PathBuf,
    pub(crate) defaults: RunnerArgs,
    pub(crate) subject: Option<Headless>,
    /// `test.stub_run`'s answers, by program, as JSON: they outlive a load.
    stubs: BTreeMap<String, String>,
    /// The nodes a spec has been handed, by the number it was handed.
    handles: Vec<NodeHandle>,
    numbers: HashMap<NodeHandle, i64>,
    /// Lines the current test wants printed with its result.
    pub(crate) notes: Vec<String>,
    /// How many of the subject's log lines a spec has already cleared.
    logs_cleared: usize,
    /// Where the current test's log lines begin.
    test_logs: usize,
    /// The subject's effect errors when the current test began.
    test_effect_errors: u64,
    /// Why there is no GPU, once that is known, so a spec full of
    /// snapshots asks the driver once.
    no_gpu: Option<String>,
    /// Frames `test.frame` rendered, by the number each was handed out as.
    frames: Vec<morf_host::headless::render::Picture>,
    started: Instant,
}

type Shared = Rc<RefCell<TestHost>>;

impl TestHost {
    pub(crate) fn new(spec_dir: PathBuf, defaults: RunnerArgs) -> Self {
        Self {
            spec_dir,
            defaults,
            subject: None,
            stubs: BTreeMap::new(),
            handles: Vec::new(),
            numbers: HashMap::new(),
            notes: Vec::new(),
            logs_cleared: 0,
            test_logs: 0,
            test_effect_errors: 0,
            no_gpu: None,
            frames: Vec::new(),
            started: Instant::now(),
        }
    }

    pub(crate) fn subject(&mut self) -> Result<&mut Headless, String> {
        self.subject
            .as_mut()
            .ok_or_else(|| "nothing is loaded: call test.load first".to_owned())
    }

    pub(crate) fn number(&mut self, node: NodeHandle) -> i64 {
        if let Some(number) = self.numbers.get(&node) {
            return *number;
        }
        self.handles.push(node);
        let number = self.handles.len() as i64;
        self.numbers.insert(node, number);
        number
    }

    pub(crate) fn handle(&self, number: i64) -> Result<NodeHandle, String> {
        usize::try_from(number - 1)
            .ok()
            .and_then(|index| self.handles.get(index).copied())
            .ok_or_else(|| format!("no node {number}"))
    }

    /// Forgets the last test's notes, and marks where its log lines end so
    /// a failure reports only its own.
    pub(crate) fn start_test(&mut self) {
        self.notes.clear();
        self.test_logs = self
            .subject
            .as_ref()
            .map_or(0, |subject| subject.logs.len());
        self.test_effect_errors = self
            .subject
            .as_ref()
            .map_or(0, |subject| subject.runtime.effect_errors());
    }

    /// How many errors the subject's effects raised during the current test.
    pub(crate) fn effect_errors(&self) -> u64 {
        self.subject.as_ref().map_or(0, |subject| {
            subject
                .runtime
                .effect_errors()
                .saturating_sub(self.test_effect_errors)
        })
    }

    /// The subject's error and warning lines, for a failed test's report.
    pub(crate) fn recent_problems(&self, limit: usize) -> Vec<String> {
        let Some(subject) = &self.subject else {
            return Vec::new();
        };
        let lines = subject
            .logs
            .iter()
            .skip(self.test_logs)
            .filter(|entry| entry.level >= LogLevel::Warn)
            .map(|entry| format!("{}: {}", entry.level.name(), entry.message))
            .collect::<Vec<_>>();
        let skip = lines.len().saturating_sub(limit);
        lines.into_iter().skip(skip).collect()
    }
}

/// Installs every host function into the spec's runtime.
pub(crate) fn install(runtime: &mut Runtime, host: &Shared) {
    type Handler = fn(&mut TestHost, &[IpcValue]) -> Result<Vec<IpcValue>, String>;
    let handlers: [(&'static str, Handler); 38] = [
        ("load", load),
        ("approve", approve),
        ("reload", reload),
        ("problems", |host, _| {
            let problems = host.subject()?.problems.iter().cloned();
            let list = problems.map(IpcValue::String).collect();
            Ok(vec![IpcValue::Table(Arc::new(IpcTable::List(list)))])
        }),
        ("surfaces", |host, _| surfaces(host)),
        ("now", |host, _| {
            let now = host.subject()?.now();
            Ok(vec![IpcValue::Integer(now.as_millis() as i64)])
        }),
        ("shortcuts_inhibited", |host, _| {
            let held = host.subject()?.runtime.shortcuts_inhibited();
            Ok(vec![IpcValue::Boolean(held)])
        }),
        ("wall_ms", |host, _| {
            Ok(vec![IpcValue::Integer(
                host.started.elapsed().as_millis() as i64
            )])
        }),
        ("advance", |host, arguments| {
            let virtual_ms = number(arguments.first(), "ms")?.max(0.0);
            let real_ms = number(arguments.get(1), "real ms").unwrap_or(0.0).max(0.0);
            host.subject()?.advance(
                Duration::from_secs_f64(virtual_ms / 1000.0),
                Duration::from_secs_f64(real_ms / 1000.0),
            );
            Ok(Vec::new())
        }),
        ("settle", |host, arguments| {
            let limit = number(arguments.first(), "ms")?.max(0.0);
            let took = host
                .subject()?
                .settle(Duration::from_secs_f64(limit / 1000.0));
            Ok(vec![IpcValue::Integer(took.as_millis() as i64)])
        }),
        ("nodes", |host, _| nodes(host)),
        ("accessible", |host, _| accessible(host)),
        ("accessible_action", accessible_action),
        ("resize_window", resize_window),
        ("text_of", text_of),
        ("click", click),
        ("button", press),
        ("move", motion),
        ("touch", touch),
        ("leave", leave),
        ("wheel", wheel),
        ("key", key),
        ("type", type_text),
        ("ipc", ipc),
        ("frames", frames),
        ("ipc_verbs", |host, _| {
            let verbs = host
                .subject()?
                .runtime
                .ipc_verbs()
                .into_iter()
                .filter(|verb| !verb.starts_with("__morf_test."))
                .map(IpcValue::String)
                .collect();
            Ok(vec![IpcValue::Table(Arc::new(IpcTable::List(verbs)))])
        }),
        ("logs", logs),
        ("clear_logs", |host, _| {
            host.logs_cleared = host.subject()?.logs.len();
            // A spec that clears the log has read what it expected there.
            host.test_effect_errors = host.subject()?.runtime.effect_errors();
            Ok(Vec::new())
        }),
        ("stub_run", stub_run),
        ("clear_stubs", |host, _| {
            host.stubs.clear();
            if let Some(subject) = &mut host.subject {
                subject
                    .runtime
                    .call_ipc("__morf_test.clear_stubs", &[])
                    .map_err(|error| error.to_string())?;
            }
            Ok(Vec::new())
        }),
        ("runs", |host, _| {
            host.subject()?
                .runtime
                .call_ipc("__morf_test.runs", &[])
                .map_err(|error| error.to_string())
        }),
        ("snapshot", snapshot),
        ("frame", pixels::frame),
        ("pixel", pixels::pixel),
        ("region", pixels::region),
        ("note", |host, arguments| {
            host.notes
                .push(optional_text(arguments.first()).unwrap_or_default());
            Ok(Vec::new())
        }),
        ("resources", |host, _| resources(host)),
        ("screen", |host, _| {
            let (width, height) = host.subject()?.screen;
            Ok(vec![
                IpcValue::Integer(i64::from(width)),
                IpcValue::Integer(i64::from(height)),
            ])
        }),
    ];
    for (name, handler) in handlers {
        let host = Rc::clone(host);
        runtime.register_host_function(
            "__morf_test",
            name,
            Rc::new(move |arguments| handler(&mut host.borrow_mut(), &arguments)),
        );
    }
}

/// What the configuration under test holds open: `test.resources()`.
fn resources(host: &mut TestHost) -> Result<Vec<IpcValue>, String> {
    let counts = host.subject()?.runtime.resource_counts();
    let count = |n: usize| IpcValue::Integer(i64::try_from(n).unwrap_or(i64::MAX));
    Ok(vec![values::map(vec![
        ("dbus_subscriptions", count(counts.dbus_subscriptions)),
        ("dbus_calls", count(counts.dbus_calls)),
        ("timers", count(counts.timers)),
        ("processes", count(counts.processes)),
        ("connections", count(counts.connections)),
        ("file_watches", count(counts.file_watches)),
        ("owned_hooks", count(counts.owned_hooks)),
    ])])
}
