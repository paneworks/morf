//! The host half of `morf.test`: the functions a spec calls, each acting on
//! the configuration under test.
//!
//! A spec's runtime and the configuration's are two runtimes. The spec calls
//! `__morf_test.click(...)`, which lands here, which drives the other one
//! through [`Headless`] -- the same code the shell's own loop runs.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use morf_host::morf_lua::{LogLevel, Runtime};
use morf_value::{IpcTable, IpcValue};
use morf_host::morf_scene::NodeHandle;

use morf_host::headless::{Headless, LoadOptions};
use crate::runner_args::RunnerArgs;
use crate::test_host_input::{
    accessible, accessible_action, click, resize_window, key, leave, motion, nodes, press, text_of, type_text, wheel,
};

const SUBJECT: &str = include_str!("test_subject.lua");

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
    /// Why there is no GPU, once that is known, so a spec full of
    /// snapshots asks the driver once.
    no_gpu: Option<String>,
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
            no_gpu: None,
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

// ------------------------------------------------------------------ values

pub(crate) fn text(value: &IpcValue) -> Option<String> {
    match value {
        IpcValue::String(text) => Some(text.clone()),
        IpcValue::Integer(number) => Some(number.to_string()),
        IpcValue::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

pub(crate) fn number(value: Option<&IpcValue>, what: &str) -> Result<f64, String> {
    match value {
        Some(IpcValue::Integer(number)) => Ok(*number as f64),
        Some(IpcValue::Number(number)) if number.is_finite() => Ok(*number),
        other => Err(format!("{what} must be a number, not {other:?}")),
    }
}

pub(crate) fn optional_text(value: Option<&IpcValue>) -> Option<String> {
    value.and_then(text)
}

fn field<'a>(table: Option<&'a IpcValue>, name: &str) -> Option<&'a IpcValue> {
    match table {
        Some(IpcValue::Table(table)) => match &**table {
            IpcTable::Map(fields) => fields.get(name),
            IpcTable::List(_) => None,
        },
        _ => None,
    }
}

pub(crate) fn list(value: Option<&IpcValue>) -> Vec<IpcValue> {
    match value {
        Some(IpcValue::Table(table)) => match &**table {
            IpcTable::List(items) => items.clone(),
            IpcTable::Map(fields) if fields.is_empty() => Vec::new(),
            IpcTable::Map(_) => Vec::new(),
        },
        _ => Vec::new(),
    }
}

pub(crate) fn map(fields: Vec<(&str, IpcValue)>) -> IpcValue {
    IpcValue::Table(Arc::new(IpcTable::Map(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )))
}

pub(crate) fn string(text: impl Into<String>) -> IpcValue {
    IpcValue::String(text.into())
}

/// A long bracket that `text` cannot end early.
fn long_string(text: &str) -> String {
    let mut level = 1;
    while text.contains(&format!("]{}]", "=".repeat(level))) {
        level += 1;
    }
    let equals = "=".repeat(level);
    format!("[{equals}[{text}]{equals}]")
}

// ------------------------------------------------------------------- load

fn load(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let options_value = arguments.get(1);
    let source = optional_text(field(options_value, "source"));
    let path = match (optional_text(arguments.first()), &source) {
        (Some(path), _) => morf_host::headless::resolve(&path, Some(&host.spec_dir)),
        (None, Some(_)) => host.spec_dir.join("inline.lua"),
        (None, None) => return Err("test.load wants a path or { source = ... }".to_owned()),
    };
    let mut options = LoadOptions::new(path.clone());
    options.source = source.map(String::into_bytes);
    options.size = host.defaults.size;
    options.scale = host.defaults.scale as i32;
    options.policy = host.defaults.policy;
    if let Some(IpcValue::Table(size)) = field(options_value, "size")
        && let IpcTable::List(items) = &**size
    {
        options.size = (
            number(items.first(), "size[1]")?.max(1.0) as u32,
            number(items.get(1), "size[2]")?.max(1.0) as u32,
        );
    }
    if let Some(screens) = field(options_value, "screens") {
        // Zero is every output gone: the configuration runs outputless.
        options.screens = number(Some(screens), "screens")?.clamp(0.0, 16.0) as usize;
    }
    options.args = list(field(options_value, "args"))
        .iter()
        .filter_map(text)
        .collect();
    let env = match field(options_value, "env") {
        Some(value) => value.to_json(),
        None => serde_json::json!({}),
    };
    let stubs = host
        .stubs
        .iter()
        .map(|(name, json)| {
            let value = serde_json::from_str::<serde_json::Value>(json).unwrap_or_default();
            (name.clone(), value)
        })
        .collect::<serde_json::Map<_, _>>();
    let data = serde_json::json!({ "stubs": stubs, "env": env }).to_string();
    options.prelude = Some(SUBJECT.replace("__MORF_TEST_DATA__", &long_string(&data)));
    // The old subject goes first: its children are killed and reaped
    // before the next configuration starts its own.
    host.subject = None;
    host.handles.clear();
    host.numbers.clear();
    host.logs_cleared = 0;
    host.test_logs = 0;
    let subject = Headless::load(&options).map_err(|failure| {
        let mut message = format!("{} did not load: {}", path.display(), failure.error);
        for entry in failure
            .logs
            .iter()
            .filter(|entry| entry.level >= LogLevel::Warn)
        {
            message.push_str("\n    ");
            message.push_str(&entry.message);
        }
        message
    })?;
    host.subject = Some(subject);
    surfaces(host)
}

fn surfaces(host: &mut TestHost) -> Result<Vec<IpcValue>, String> {
    let subject = host.subject()?;
    let rows = subject
        .surfaces
        .iter()
        .map(|surface| {
            map(vec![
                ("label", string(surface.label())),
                ("kind", string(surface.kind)),
                ("name", string(surface.name.clone())),
                ("width", IpcValue::Integer(i64::from(surface.size.0))),
                ("height", IpcValue::Integer(i64::from(surface.size.1))),
                ("x", IpcValue::Integer(i64::from(surface.position.0))),
                ("y", IpcValue::Integer(i64::from(surface.position.1))),
                ("visible", IpcValue::Boolean(surface.visible)),
            ])
        })
        .collect();
    Ok(vec![IpcValue::Table(Arc::new(IpcTable::List(rows)))])
}

// ------------------------------------------------------------ the subject

fn ipc(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let verb = optional_text(arguments.first()).ok_or("test.ipc wants a verb")?;
    let subject = host.subject()?;
    let result = subject
        .runtime
        .call_ipc(&verb, &arguments[1..])
        .map_err(|error| format!("ipc {verb}: {error}"))?;
    // As the shell would paint after the call: effects flushed, a layout.
    subject.frame(Duration::ZERO);
    Ok(result)
}

fn logs(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let level = optional_text(arguments.first())
        .and_then(|name| LogLevel::parse(&name))
        .unwrap_or(LogLevel::Debug);
    let skip = host.logs_cleared;
    let subject = host.subject()?;
    let rows = subject
        .logs
        .iter()
        .skip(skip)
        .filter(|entry| entry.level >= level)
        .map(|entry| {
            map(vec![
                ("level", string(entry.level.name())),
                ("message", string(entry.message.clone())),
            ])
        })
        .collect();
    Ok(vec![IpcValue::Table(Arc::new(IpcTable::List(rows)))])
}

fn stub_run(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let program = optional_text(arguments.first()).ok_or("test.stub_run wants a program")?;
    let answer = arguments
        .get(1)
        .map(IpcValue::to_json)
        .unwrap_or(serde_json::Value::Null)
        .to_string();
    host.stubs.insert(program.clone(), answer.clone());
    if let Some(subject) = &mut host.subject {
        subject
            .runtime
            .call_ipc("__morf_test.stub", &[string(program), string(answer)])
            .map_err(|error| error.to_string())?;
    }
    Ok(Vec::new())
}

fn snapshot(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let name = optional_text(arguments.first()).ok_or("test.snapshot wants a file name")?;
    if let Some(reason) = &host.no_gpu {
        let note = format!("snapshot {name} skipped: {reason}");
        host.notes.push(note.clone());
        return Ok(vec![IpcValue::Boolean(false), string(note)]);
    }
    let directory = host
        .defaults
        .snapshots
        .clone()
        .unwrap_or_else(|| host.spec_dir.join("snapshots"));
    let path = if Path::new(&name).is_absolute() {
        PathBuf::from(&name)
    } else {
        directory.join(&name)
    };
    let scale = host.defaults.scale;
    let wanted = optional_text(arguments.get(1));
    let subject = host.subject()?;
    match morf_host::headless_render::render_to(subject, wanted.as_deref(), scale, &path) {
        Ok(_) => Ok(vec![
            IpcValue::Boolean(true),
            string(path.display().to_string()),
        ]),
        Err(error) if error.starts_with("no GPU") => {
            host.notes.push(format!("snapshot {name} skipped: {error}"));
            host.no_gpu = Some(error.clone());
            Ok(vec![IpcValue::Boolean(false), string(error)])
        }
        Err(error) => Err(error),
    }
}

/// Installs every host function into the spec's runtime.
pub(crate) fn install(runtime: &mut Runtime, host: &Shared) {
    type Handler = fn(&mut TestHost, &[IpcValue]) -> Result<Vec<IpcValue>, String>;
    let handlers: [(&'static str, Handler); 29] = [
        ("load", load),
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
        ("leave", leave),
        ("wheel", wheel),
        ("key", key),
        ("type", type_text),
        ("ipc", ipc),
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
        ("note", |host, arguments| {
            host.notes
                .push(optional_text(arguments.first()).unwrap_or_default());
            Ok(Vec::new())
        }),
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
