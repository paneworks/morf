//! The configuration under test as a whole: loading it, its surfaces, its
//! IPC and logs, the programs it runs, and pictures of it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use morf_app::Backend;
use morf_host::headless::{Headless, LoadOptions};
use morf_lua::LogLevel;
use morf_value::{IpcTable, IpcValue};

use super::TestHost;
use super::values::{field, list, map, number, optional_text, string, text};

const SUBJECT: &str = include_str!("../test_subject.lua");

/// A long bracket that `text` cannot end early.
fn long_string(text: &str) -> String {
    let mut level = 1;
    while text.contains(&format!("]{}]", "=".repeat(level))) {
        level += 1;
    }
    let equals = "=".repeat(level);
    format!("[{equals}[{text}]{equals}]")
}

pub(super) fn load(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
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
    host.test_effect_errors = 0;
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

pub(super) fn surfaces(host: &mut TestHost) -> Result<Vec<IpcValue>, String> {
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

/// Configures a window to a new size, as a compositor does when a person
/// resizes it or a phone fits it to the screen: `(surface label, w, h)`.
pub(super) fn resize_window(
    host: &mut TestHost,
    arguments: &[IpcValue],
) -> Result<Vec<IpcValue>, String> {
    let label = arguments
        .first()
        .and_then(text)
        .ok_or("resize_window wants a surface")?;
    let width = number(arguments.get(1), "width")?.max(1.0) as u32;
    let height = number(arguments.get(2), "height")?.max(1.0) as u32;
    let subject = host.subject()?;
    let Some(surface) = subject
        .surfaces
        .iter_mut()
        .find(|s| s.label() == label || s.name == label)
    else {
        return Err(format!("no surface {label}"));
    };
    let Some(id) = surface.id else {
        return Err(format!("{label} is not a window"));
    };
    let role = surface.role;
    if !surface.open {
        surface.size = (width, height);
        let changed = subject.runtime.set_window_surface_size(id, width, height);
        return Ok(vec![IpcValue::Boolean(changed)]);
    }
    // An open window is resized as a compositor does it: a configure.
    let changed = surface.size != (width, height);
    if let Some(backend) = subject
        .host
        .as_mut()
        .and_then(|host| host.backend.as_headless_mut())
    {
        backend.resize(role, (width, height));
    }
    subject.frame(std::time::Duration::ZERO);
    Ok(vec![IpcValue::Boolean(changed)])
}

pub(super) fn ipc(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
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

/// `test.frames()`: the host's frame accounting since the last call, and
/// a new window begun.
pub(super) fn frames(host: &mut TestHost, _: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let report = host
        .subject()?
        .host
        .as_mut()
        .ok_or("the configuration has no surface, so no frames")?
        .take_frame_report();
    let ms = |took: Duration| IpcValue::Number(took.as_secs_f64() * 1000.0);
    let count = |count: u64| IpcValue::Integer(count as i64);
    let owners = morf_host::host::turn::OWNERS;
    let stages = owners
        .iter()
        .zip(report.stages)
        .map(|(name, took)| (*name, ms(took)));
    let held = owners
        .iter()
        .zip(report.owners)
        .map(|(name, held)| (*name, count(held)));
    Ok(vec![map(vec![
        ("frames", count(report.frames)),
        ("late", count(report.late)),
        ("overruns", count(report.overruns)),
        ("turns", count(report.turns)),
        ("motion_turns", count(report.motion_turns)),
        ("worst_ms", ms(report.worst)),
        ("mean_ms", ms(report.mean())),
        ("refresh_ms", ms(report.refresh)),
        ("stages", map(stages.collect())),
        ("owners", map(held.collect())),
        ("summary", string(report.to_string())),
    ])])
}

pub(super) fn logs(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
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

pub(super) fn stub_run(
    host: &mut TestHost,
    arguments: &[IpcValue],
) -> Result<Vec<IpcValue>, String> {
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

pub(super) fn snapshot(
    host: &mut TestHost,
    arguments: &[IpcValue],
) -> Result<Vec<IpcValue>, String> {
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
    match morf_host::headless::render::render_to(subject, wanted.as_deref(), scale, &path) {
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
