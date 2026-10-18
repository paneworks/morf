//! An imported module's declaration in plain words, as `morf check` and
//! `morf approve` both print it.

use morf_lua::{Approval, Grant, ModuleDeclaration, ModuleKind, ServiceEntry, unknown_service};

/// The line every surface prints for a grant that can run anything.
pub(crate) const EVERYTHING: &str = "runs anything it is told: this asks for everything.";

/// Programs that run whatever they are handed: shells, language
/// interpreters, and the launchers and wrappers that exec their arguments.
const INTERPRETERS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "dash",
    "fish",
    "ksh",
    "mksh",
    "csh",
    "tcsh",
    "nu",
    "elvish",
    "xonsh",
    "python",
    "python2",
    "python3",
    "pypy",
    "pypy3",
    "perl",
    "ruby",
    "node",
    "nodejs",
    "deno",
    "bun",
    "lua",
    "luajit",
    "php",
    "tclsh",
    "wish",
    "awk",
    "gawk",
    "mawk",
    "nawk",
    "guile",
    "racket",
    "osascript",
    "pwsh",
    "env",
    "busybox",
    "toybox",
    "xargs",
    "exec",
    "eval",
    "nohup",
    "setsid",
    "timeout",
    "nice",
    "ionice",
    "chrt",
    "taskset",
    "stdbuf",
    "unbuffer",
    "expect",
    "script",
    "sudo",
    "doas",
    "pkexec",
    "su",
    "runuser",
    "chroot",
    "unshare",
    "nsenter",
    "systemd-run",
    "flatpak-spawn",
    "uwsm",
    "app2unit",
    "gtk-launch",
    "xdg-open",
    "gio",
    "dbus-launch",
    "ssh",
    "make",
    "morf",
    "kitty",
    "foot",
    "footclient",
    "alacritty",
    "wezterm",
    "xterm",
    "ghostty",
    "konsole",
    "terminator",
    "st",
    "urxvt",
];

/// Whether `program` (a name or a path, perhaps versioned like
/// `python3.12`) is one of [`INTERPRETERS`].
pub(crate) fn is_interpreter(program: &str) -> bool {
    let name = program.rsplit('/').next().unwrap_or(program);
    let bare = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    INTERPRETERS.contains(&name) || INTERPRETERS.contains(&bare)
}

/// Whether the declaration grants running any program, or one that runs
/// anything.
pub(crate) fn asks_for_everything(record: &ModuleDeclaration) -> bool {
    ["run", "process"]
        .iter()
        .filter_map(|group| record.grants.get(*group))
        .any(|grant| match grant {
            Grant::Any => true,
            Grant::Names(names) => names.iter().any(|name| is_interpreter(name)),
        })
}

fn group_words(group: &str, grant: &Grant) -> String {
    let (any, some) = match group {
        "run" | "process" => ("run any program", "run"),
        "fs" => ("write files outside its own folder", "write"),
        "dbus" => (
            "talk to anything on the message bus",
            "talk on the message bus to",
        ),
        "http" => ("make HTTP requests anywhere", "make HTTP requests to"),
        "socket" => ("open any socket", "open sockets to"),
        "pam" => ("check passwords (PAM)", "check passwords (PAM) for"),
        "greetd" => (
            "start login sessions (greetd)",
            "start login sessions (greetd) for",
        ),
        "services" => ("use any system service", "use the system services"),
        "system" => (
            "talk to the operating system",
            "talk to the operating system's",
        ),
        "desktop" => (
            "control the compositor and desktop",
            "control the desktop's",
        ),
        "media" => ("control media and audio", "control media and audio:"),
        "io" => ("do input and output", "do input and output with"),
        other => return format!("ask for `{other}`, which morf does not know"),
    };
    match grant {
        Grant::Any => any.to_owned(),
        Grant::Names(names) => format!("{some} {}", names.join(", ")),
    }
}

/// The services a `services` grant names, each with what it holds as the
/// catalog says, when there is a catalog to ask.
fn services_words(names: &[String], catalog: Option<&[ServiceEntry]>) -> String {
    let named: Vec<String> = names
        .iter()
        .map(|name| {
            let entry = catalog.and_then(|catalog| catalog.iter().find(|e| &e.name == name));
            match entry {
                Some(entry) if !entry.holds.is_empty() => format!("{name} ({})", entry.holds),
                _ => name.clone(),
            }
        })
        .collect();
    format!("use the system services {}", named.join(", "))
}

/// The names a module lists under `services` that the catalog does not
/// hold, each as the registry would refuse it.
pub(crate) fn unknown_services(
    record: &ModuleDeclaration,
    catalog: Option<&[ServiceEntry]>,
) -> Vec<String> {
    let (Some(catalog), Some(Grant::Names(names))) = (catalog, record.grants.get("services"))
    else {
        return Vec::new();
    };
    names
        .iter()
        .filter_map(|name| unknown_service(name, catalog))
        .collect()
}

/// The module's name, kind and file, then one line per grant, indented;
/// `catalog` words the services it names.
pub(crate) fn describe(
    record: &ModuleDeclaration,
    catalog: Option<&[ServiceEntry]>,
) -> Vec<String> {
    let kind = match record.kind {
        ModuleKind::Module => "module",
        ModuleKind::Plugin => "plugin",
    };
    let mut lines = vec![format!(
        "{kind} {} ({})",
        record.name,
        record.path.display()
    )];
    if let Some(problem) = &record.problem {
        lines.push(format!("  its manifest cannot be read: {problem}"));
    } else if !record.declared() {
        lines.push("  declares nothing; approving covers the code as it stands".to_owned());
    } else if record.grants.is_empty() {
        lines.push("  asks for nothing".to_owned());
    }
    for (group, grant) in &record.grants {
        let words = match (group.as_str(), grant) {
            ("services", Grant::Names(names)) => services_words(names, catalog),
            _ => group_words(group, grant),
        };
        lines.push(format!("  may {words}"));
    }
    for unknown in unknown_services(record, catalog) {
        lines.push(format!("  {unknown}"));
    }
    if asks_for_everything(record) {
        lines.push(format!("  {EVERYTHING}"));
    }
    if !record.loaded {
        lines.push("  (found in a source, not loaded at start)".to_owned());
    }
    lines
}

/// What `morf check` says about a run's imported modules and plugins.
#[derive(Debug, Default)]
pub(crate) struct Report {
    pub(crate) lines: Vec<String>,
    pub(crate) errors: Vec<String>,
}

/// Each imported module and plugin the run met, with where it stands and
/// what it declares; a manifest that cannot be read or that lists a
/// service the catalog does not hold is an error, and so, under `strict`,
/// is an imported integration with no manifest.
pub(crate) fn report(runtime: &mut morf_lua::Runtime, strict: bool) -> Report {
    let mut report = Report::default();
    let catalog = runtime.service_catalog().ok();
    let catalog = catalog.as_deref();
    let approvals = runtime
        .approvals_path()
        .and_then(|path| morf_lua::Approvals::load(&path))
        .unwrap_or_default();
    for record in runtime.module_declarations() {
        if !record.imported() {
            continue;
        }
        let mut lines = describe(&record, catalog).into_iter();
        if let Some(head) = lines.next() {
            let status = status_words(approvals.status(&record));
            report.lines.push(format!("imported {head}: {status}"));
        }
        report.lines.extend(lines);
        if let Some(problem) = &record.problem {
            report.errors.push(format!("{}: {problem}", record.name));
        }
        for unknown in unknown_services(&record, catalog) {
            report.errors.push(format!("{}: {unknown}", record.name));
        }
        if strict && record.is_integration() && !record.declared() {
            report.errors.push(format!(
                "imported integration {} declares no permissions ({} is missing)",
                record.name,
                morf_lua::manifest_path(&record.path).display()
            ));
        }
        for group in record.unknown_groups() {
            report
                .lines
                .push(format!("  warning: `{group}` is not a permission group"));
        }
    }
    report
}

/// Where a module stands, as a word or two.
pub(crate) fn status_words(status: Approval) -> &'static str {
    match status {
        Approval::Approved => "approved",
        Approval::Lapsed => "changed since it was approved",
        Approval::Unknown => "not approved",
    }
}
