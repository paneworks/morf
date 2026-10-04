//! `morf test`: spec files, run one test at a time, reported as TAP.
//!
//! Each spec file gets a runtime of its own with `morf.test` in it. Running
//! the file registers its tests; each is then run by name, through a hidden
//! IPC verb the prelude installs, so a test that fails -- an assertion, a
//! Lua error, a configuration that would not load -- fails alone.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

use morf_lua::{Limits, Runtime};
use morf_value::{IpcTable, IpcValue};

use crate::runner_args::RunnerArgs;
use crate::test_host::{TestHost, install};

const PRELUDE: &str = include_str!("test_prelude.lua");

/// A spec's budget: a hundred times a handler's, because one test does in a
/// call what a shell spreads over minutes -- and a loop that never ends is
/// still stopped, in seconds rather than never.
fn spec_limits() -> Limits {
    Limits {
        fuel: 100_000_000,
        effect_fuel: 100_000_000,
        frame_fuel: 100_000_000,
        ..Limits::default()
    }
}

/// One test's name, and why it is skipped when it is.
struct Case {
    index: i64,
    name: String,
    skip: Option<String>,
}

#[derive(Default)]
pub(crate) struct Tally {
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) skipped: usize,
}

fn field<'a>(value: &'a IpcValue, name: &str) -> Option<&'a IpcValue> {
    match value {
        IpcValue::Table(table) => match &**table {
            IpcTable::Map(fields) => fields.get(name),
            IpcTable::List(_) => None,
        },
        _ => None,
    }
}

fn text(value: Option<&IpcValue>) -> String {
    match value {
        Some(IpcValue::String(text)) => text.clone(),
        _ => String::new(),
    }
}

/// The tests a spec registered, in the order it registered them.
fn cases(runtime: &mut Runtime) -> Result<Vec<Case>, String> {
    let listed = runtime
        .call_ipc("__morf_test.list", &[])
        .map_err(|error| error.to_string())?;
    let Some(IpcValue::Table(table)) = listed.first() else {
        return Ok(Vec::new());
    };
    let IpcTable::List(items) = &**table else {
        return Ok(Vec::new());
    };
    Ok(items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let skip = text(field(item, "skip"));
            Case {
                index: index as i64 + 1,
                name: text(field(item, "name")),
                skip: (!skip.is_empty()).then_some(skip),
            }
        })
        .collect())
}

/// Prints a block of lines as TAP diagnostics.
fn diagnose(lines: &str) {
    for line in lines.lines() {
        println!("#   {line}");
    }
}

/// Runs one spec file, numbering its tests from `number`.
pub(crate) fn run_spec(
    path: &Path,
    args: &RunnerArgs,
    number: &mut usize,
    tally: &mut Tally,
) -> Result<(), String> {
    println!("# {}", path.display());
    let source = std::fs::read(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let spec_dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let host = Rc::new(RefCell::new(TestHost::new(spec_dir.clone(), args.clone())));
    let mut runtime = Runtime::new(spec_limits());
    // The spec's own folder, and its project's library: a spec of a library
    // module requires it as a shell would.
    let mut roots = vec![spec_dir.clone()];
    roots.extend(morf_lua::project_library(path));
    runtime.set_module_roots(roots);
    runtime.set_shell_root(spec_dir);
    install(&mut runtime, &host);
    runtime
        .execute("=morf.test", PRELUDE.as_bytes())
        .map_err(|error| format!("the test prelude: {error}"))?;
    if let Err(error) = runtime.execute(&path.to_string_lossy(), &source) {
        *number += 1;
        tally.failed += 1;
        println!("not ok {number} - {} did not load", path.display());
        diagnose(&error.to_string());
        return Ok(());
    }
    for case in cases(&mut runtime)? {
        if args
            .filter
            .as_ref()
            .is_some_and(|filter| !case.name.contains(filter.as_str()))
        {
            continue;
        }
        *number += 1;
        if let Some(reason) = &case.skip {
            tally.skipped += 1;
            println!("ok {number} - {} # SKIP {reason}", case.name);
            continue;
        }
        host.borrow_mut().start_test();
        let started = Instant::now();
        let outcome = runtime.call_ipc("__morf_test.run", &[IpcValue::Integer(case.index)]);
        let took = started.elapsed().as_secs_f64() * 1000.0;
        let (passed, message) = match &outcome {
            Ok(values) => match values.first() {
                Some(result) => (
                    matches!(field(result, "ok"), Some(IpcValue::Boolean(true))),
                    text(field(result, "message")),
                ),
                None => (false, "the test returned nothing".to_owned()),
            },
            Err(error) => (false, error.to_string()),
        };
        if passed {
            tally.passed += 1;
            println!("ok {number} - {} ({took:.0} ms)", case.name);
        } else {
            tally.failed += 1;
            println!("not ok {number} - {} ({took:.0} ms)", case.name);
            diagnose(&message);
            let problems = host.borrow().recent_problems(8);
            if !problems.is_empty() {
                println!("#   the configuration said:");
                for line in problems {
                    println!("#     {line}");
                }
            }
        }
        for note in std::mem::take(&mut host.borrow_mut().notes) {
            println!("#   {note}");
        }
    }
    // The configuration goes before the spec's runtime, so its children
    // are reaped while everything that might ask about them still exists.
    host.borrow_mut().subject = None;
    Ok(())
}

/// `morf test`: every spec file, then a summary. True when nothing failed.
pub(crate) fn run(args: &RunnerArgs) -> Result<bool, String> {
    let started = Instant::now();
    println!("TAP version 13");
    let mut number = 0;
    let mut tally = Tally::default();
    for path in &args.files {
        // Each spec file starts from an empty home, as it would alone.
        if args.isolate {
            morf_host::headless_env::empty_home(&morf_host::headless_env::scratch_dir());
        }
        if let Err(error) = run_spec(path, args, &mut number, &mut tally) {
            number += 1;
            tally.failed += 1;
            println!("not ok {number} - {}", path.display());
            diagnose(&error);
        }
    }
    println!("1..{number}");
    println!(
        "# {} passed, {} failed, {} skipped in {:.2} s",
        tally.passed,
        tally.failed,
        tally.skipped,
        started.elapsed().as_secs_f64()
    );
    Ok(tally.failed == 0)
}
