//! `morf check` and `morf render`: a configuration run once, headless, and
//! either reported on or drawn. `morf test` is in `test_runner`.

use morf_host::morf_lua::{LogEntry, LogLevel};

use morf_host::headless::{Headless, LoadOptions};
use morf_host::headless_env::{PrivateBus, isolate_from_session, isolate_home, scratch_dir};
use crate::runner_args::{Runner, RunnerArgs};

/// Runs whichever runner was asked for. Isolation first, before anything
/// starts a thread: the environment is only safely changed until then.
pub(crate) fn run(args: RunnerArgs) -> Result<(), String> {
    isolate_from_session(args.no_dbus);
    let scratch = scratch_dir();
    let mut made_scratch = false;
    if args.isolate {
        isolate_home(&scratch)?;
        made_scratch = true;
    }
    // Declared after the scratch folder is decided and before anything
    // runs, so it is stopped when the run ends however it ends.
    let bus = if args.private_bus && !args.no_dbus {
        made_scratch = true;
        Some(PrivateBus::start(&scratch.join("bus"))?)
    } else {
        None
    };
    let outcome = match args.runner {
        Runner::Check => check(&args),
        Runner::Render => render(&args),
        Runner::Test => crate::test_runner::run(&args),
    };
    drop(bus);
    if made_scratch {
        let _ = std::fs::remove_dir_all(&scratch);
    }
    match outcome {
        Ok(true) => Ok(()),
        // The report has been printed; the exit status is what is left to say.
        Ok(false) => std::process::exit(1),
        Err(error) => Err(error),
    }
}

/// What a check found, sorted into what fails it and what only might.
#[derive(Default)]
pub(crate) struct Findings {
    pub(crate) errors: Vec<String>,
    pub(crate) warnings: Vec<String>,
}

/// Whether a log line is a Lua error rather than advice.
///
/// The runtime logs a failed callback, binding or handler at warning level,
/// so the shell stays up; a check holds it to a higher standard. A line is
/// an error when it was logged as one, when it carries a Lua source
/// position, or when it names a callback, handler or loader that failed.
pub(crate) fn is_error(entry: &LogEntry) -> bool {
    if entry.level >= LogLevel::Error {
        return true;
    }
    if entry.level < LogLevel::Warn || entry.message.starts_with("lint:") {
        return false;
    }
    let message = entry.message.as_str();
    let located = message.match_indices(".lua:").any(|(at, _)| {
        message[at + 5..]
            .chars()
            .next()
            .is_some_and(|next| next.is_ascii_digit())
    });
    located
        || [
            "callback:",
            "handler:",
            "binding:",
            "Loader:",
            "view:",
            "on_finished:",
            "on_destroyed:",
            "runtime error",
        ]
        .iter()
        .any(|marker| message.contains(marker))
}

/// Sorts everything a headless run said into errors and warnings.
pub(crate) fn findings(headless: &Headless) -> Findings {
    let mut found = Findings::default();
    for problem in &headless.problems {
        found.errors.push(problem.clone());
    }
    for entry in headless.logs_at(LogLevel::Warn) {
        let line = entry.message.clone();
        if is_error(entry) {
            found.errors.push(line);
        } else if !found.warnings.contains(&line) {
            found.warnings.push(line);
        }
    }
    for surface in &headless.surfaces {
        if !surface.stable {
            found.warnings.push(format!(
                "{}: layout still moving after every pass: a binding feeds its own geometry back",
                surface.label()
            ));
        }
    }
    for line in headless.runtime.binding_dependencies() {
        found.warnings.push(format!("binding: {line}"));
    }
    found
}

/// Makes the `--ipc` calls a check or a render was asked for, each followed
/// by a settle, as a person would call them one after another.
fn call_ipc(headless: &mut Headless, args: &RunnerArgs) -> Result<(), String> {
    for call in &args.ipc {
        let values = call[1..]
            .iter()
            .map(|word| morf_value::IpcValue::String(word.clone()))
            .collect::<Vec<_>>();
        headless
            .runtime
            .call_ipc(&call[0], &values)
            .map_err(|error| format!("--ipc {}: {error}", call.join(" ")))?;
        headless.settle(std::time::Duration::from_secs(5));
    }
    Ok(())
}

/// `--a11y`: what a screen reader would be told of every shown surface.
/// Every control it could reach -- one that takes focus, or plays a role
/// that is read by name -- must have a name. Returns how many have none.
fn check_a11y(headless: &Headless) -> usize {
    const NAMED: &[&str] = &[
        "button", "toggle_button", "check_box", "radio_button", "switch", "link", "menu_item", "menu_item_check",
        "menu_item_radio", "slider", "spin_button", "tab", "text_field", "password_text", "text_area",
        "search_field", "list_box_option", "image", "dialog", "alert_dialog", "splitter",
    ];
    let scene = headless.runtime.scene();
    let mut problems = Vec::new();
    let mut nodes = 0;
    for surface in headless.surfaces.iter().filter(|s| s.visible) {
        let Some(layout) = &surface.layout else { continue };
        let tree = scene.accessible_tree(surface.root, "window", &surface.label(), &|node| {
            layout.surface_rect(&scene, node).map(|g| (g.x, g.y, g.width, g.height))
        });
        nodes += tree.len();
        for item in tree.iter().skip(1) {
            if item.name.is_empty() && (item.focusable || NAMED.contains(&item.role.as_str())) {
                let id = scene.string_value(item.node, "id").unwrap_or("");
                let id = if id.is_empty() { "no id".to_owned() } else { id.to_owned() };
                problems.push(format!("{}: a {} with no name ({id})", surface.label(), item.role));
            }
        }
    }
    problems.sort();
    problems.dedup();
    println!("  a11y    {nodes} accessible nodes, {} unnamed", problems.len());
    for problem in &problems {
        println!("  error: a11y: {problem}");
    }
    problems.len()
}

/// `--kit`: the configuration's `kit` module against the widget contract.
/// Prints the contract's stage, how much is due and every problem;
/// returns how many problems there were.
fn check_kit(headless: &mut Headless) -> usize {
    const CHECK: &[u8] = br#"
        local morf = require("morf")
        morf.ipc["morf-kit-check"] = function()
          local check = require("lib.kit.check")
          local ok, kit = pcall(require, "kit")
          if not ok then return "no kit module: " .. tostring(kit) end
          local summary = check.summary(kit)
          local out = { ("stage %d: %d due, %d missing"):format(summary.stage, summary.due, summary.missing) }
          for _, problem in ipairs(check.kit(kit)) do out[#out + 1] = problem end
          for _, problem in ipairs(check.catalogue()) do out[#out + 1] = problem end
          return table.unpack(out)
        end
    "#;
    if let Err(error) = headless.runtime.execute("morf-kit-check", CHECK) {
        println!("  error: kit: {error}");
        return 1;
    }
    let lines = match headless.runtime.call_ipc("morf-kit-check", &[]) {
        Ok(values) => values,
        Err(error) => {
            println!("  error: kit: {error}");
            return 1;
        }
    };
    let text = |value: &morf_value::IpcValue| match value {
        morf_value::IpcValue::String(text) => text.clone(),
        other => format!("{other:?}"),
    };
    let mut lines = lines.iter().map(text);
    let head = lines.next().unwrap_or_default();
    if !head.starts_with("stage ") {
        println!("  error: kit: {head}");
        return 1;
    }
    println!("  kit     {head}");
    let problems = lines.collect::<Vec<_>>();
    for problem in &problems {
        println!("  error: kit: {problem}");
    }
    problems.len()
}

/// Counts the nodes under a root, itself included.
fn node_count(headless: &Headless, root: morf_host::morf_scene::NodeHandle) -> usize {
    let scene = headless.runtime.scene();
    let mut count = 0;
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        count += 1;
        if let Ok(children) = scene.children(node) {
            pending.extend(children.iter().copied());
        }
    }
    count
}

fn options(args: &RunnerArgs, screen_index: usize) -> LoadOptions {
    let mut options = LoadOptions::new(args.files[0].clone());
    options.size = args.size;
    options.screens = args.screens;
    options.screen_index = screen_index;
    options.scale = args.scale as i32;
    options.args = args.args.clone();
    options.policy = args.policy;
    options
}

/// `morf check`: loads, runs a little, lays everything out, and says what
/// it found. True when nothing failed it.
fn check(args: &RunnerArgs) -> Result<bool, String> {
    let config = args.files[0].display().to_string();
    let (mut errors, mut warnings) = (0usize, 0usize);
    // `--screens 0`: one pass, as the shell runs it once every output is gone.
    for screen_index in 0..args.screens.max(1) {
        let options = options(args, screen_index);
        if args.screens == 0 {
            println!("{config} with no output");
        } else {
            println!(
                "{config} on HEADLESS-{} ({}x{}, {} screen{})",
                screen_index + 1,
                args.size.0,
                args.size.1,
                args.screens,
                if args.screens == 1 { "" } else { "s" }
            );
        }
        let mut headless = match Headless::load(&options) {
            Ok(headless) => headless,
            Err(failure) => {
                println!("  error: {}", failure.error);
                errors += 1;
                for entry in failure.logs.iter().filter(|entry| is_error(entry)) {
                    println!("  error: {}", entry.message);
                    errors += 1;
                }
                continue;
            }
        };
        if let Err(error) = call_ipc(&mut headless, args) {
            println!("  error: {error}");
            errors += 1;
        }
        headless.advance(args.after, args.wait);
        for surface in &headless.surfaces {
            println!(
                "  surface {:<24} {:>5}x{:<5} at {},{}{}  {} nodes",
                surface.label(),
                surface.size.0,
                surface.size.1,
                surface.position.0,
                surface.position.1,
                if surface.visible { "" } else { "  hidden" },
                node_count(&headless, surface.root),
            );
        }
        let verbs = headless.runtime.ipc_verbs();
        if !verbs.is_empty() {
            println!("  ipc     {}", verbs.join(" "));
        }
        let found = findings(&headless);
        for error in &found.errors {
            println!("  error: {error}");
        }
        for warning in &found.warnings {
            println!("  warning: {warning}");
        }
        errors += found.errors.len();
        warnings += found.warnings.len();
        if args.kit {
            errors += check_kit(&mut headless);
        }
        if args.a11y {
            errors += check_a11y(&headless);
        }
    }
    println!(
        "{errors} error{}, {warnings} warning{}",
        if errors == 1 { "" } else { "s" },
        if warnings == 1 { "" } else { "s" }
    );
    Ok(errors == 0 && (!args.strict || warnings == 0))
}

/// `morf render`: loads, runs `--after` of virtual time, and draws a
/// surface -- or the whole screen -- to a PNG.
fn render(args: &RunnerArgs) -> Result<bool, String> {
    let options = options(args, 0);
    let mut headless = Headless::load(&options).map_err(|failure| {
        let mut message = failure.error;
        for entry in failure.logs.iter().filter(|entry| is_error(entry)) {
            message.push_str("\n  ");
            message.push_str(&entry.message);
        }
        message
    })?;
    call_ipc(&mut headless, args)?;
    headless.advance(args.after, args.wait);
    let output = args.output.clone().expect("render has an output");
    let (width, height, what) = morf_host::headless_render::render_to(
        &mut headless,
        args.surface.as_deref(),
        args.scale,
        &output,
    )?;
    println!("{}: {what}, {width}x{height}", output.display());
    // The picture is what was asked for; what the configuration got wrong
    // on the way is said, but is `morf check`'s to fail on.
    for error in findings(&headless).errors {
        eprintln!("morf: {error}");
    }
    Ok(true)
}
