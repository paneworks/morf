//! What `morf check` says about imported modules -- the same plain words
//! `morf approve` uses -- and what `--strict` fails on.

use std::fs;
use std::path::{Path, PathBuf};

use morf_lua::{Grant, ModuleDeclaration, ModuleKind, Origin};

use crate::permission_words::{EVERYTHING, asks_for_everything, describe, is_interpreter};
use crate::runner_args::{Runner, parse_runner};

fn record(grants: &[(&str, Grant)]) -> ModuleDeclaration {
    let mut record = ModuleDeclaration::new(
        "plug.widget",
        PathBuf::from("/site/plug/widget.lua"),
        Origin::Imported,
        ModuleKind::Module,
    );
    record.manifest = Some(PathBuf::from("/site/plug/widget.permissions.lua"));
    record.grants = grants
        .iter()
        .cloned()
        .map(|(g, grant)| (g.to_owned(), grant))
        .collect();
    record
}

fn names(list: &[&str]) -> Grant {
    Grant::Names(list.iter().map(|name| (*name).to_owned()).collect())
}

#[test]
fn interpreters_are_named_by_any_spelling() {
    for program in [
        "sh",
        "/usr/bin/bash",
        "python3",
        "python3.12",
        "lua5.4",
        "env",
        "xargs",
    ] {
        assert!(is_interpreter(program), "{program}");
    }
    for program in ["touch", "hyprctl", "notify-send", "x264"] {
        assert!(!is_interpreter(program), "{program}");
    }
}

#[test]
fn a_grant_that_can_run_anything_says_so() {
    assert!(asks_for_everything(&record(&[("run", Grant::Any)])));
    assert!(asks_for_everything(&record(&[(
        "run",
        names(&["touch", "sh"])
    )])));
    assert!(asks_for_everything(&record(&[(
        "process",
        names(&["/usr/bin/python3"])
    )])));
    assert!(!asks_for_everything(&record(&[("run", names(&["touch"]))])));
    assert!(!asks_for_everything(&record(&[("http", Grant::Any)])));
    let text = describe(&record(&[("dbus", Grant::Any), ("run", names(&["bash"]))])).join("\n");
    assert!(
        text.contains("may talk to anything on the message bus"),
        "{text}"
    );
    assert!(text.contains("may run bash"), "{text}");
    assert!(text.contains(EVERYTHING), "{text}");
    let services = describe(&record(&[("services", names(&["media", "power"]))])).join("\n");
    assert!(
        services.contains("may use the system services media, power"),
        "{services}"
    );
}

struct Project(PathBuf);

impl Project {
    fn new(name: &str, manifest: Option<&str>) -> Self {
        let root = std::env::temp_dir().join(format!("morf-strict-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let write = |relative: &str, text: &str| {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        write(
            "config/shell.lua",
            "require('lib.integrations.bare')\nrequire('lib.fixture.runs')\nmorf.ui.Item {}",
        );
        write("library/lib/integrations/bare.lua", "return {}");
        write("library/lib/fixture/runs.lua", "return {}");
        write(
            "library/lib/fixture/runs.permissions.lua",
            "return { run = true }",
        );
        if let Some(manifest) = manifest {
            write("library/lib/integrations/bare.permissions.lua", manifest);
        }
        Self(fs::canonicalize(root).unwrap())
    }

    fn check(&self, config: &Path, strict: bool) -> bool {
        let config = config.to_string_lossy().into_owned();
        let mut words = vec![config.as_str(), "--after", "0", "--isolate"];
        if strict {
            words.push("--strict");
        }
        let args = parse_runner(Runner::Check, &words).unwrap();
        crate::runners::check(&args).unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn strict_fails_an_imported_integration_without_a_manifest() {
    let bare = Project::new("bare", None);
    let config = bare.0.join("config/shell.lua");
    assert!(
        bare.check(&config, false),
        "without --strict it is reported, not failed"
    );
    assert!(!bare.check(&config, true), "--strict fails it");
    let dotted = bare.0.join("config/../config/shell.lua");
    assert!(!bare.check(&dotted, true), "however the path is spelled");

    let declared = Project::new("declared", Some("return { http = { 'example.org' } }"));
    assert!(declared.check(&declared.0.join("config/shell.lua"), true));
}

#[test]
fn a_manifest_that_cannot_be_read_fails_the_check() {
    let broken = Project::new("broken", Some("return 'not a table'"));
    assert!(!broken.check(&broken.0.join("config/shell.lua"), false));
}

#[test]
fn check_reaches_no_bus_unless_given_one() {
    let check = parse_runner(Runner::Check, &["shell.lua"]).unwrap();
    assert!(check.no_dbus);
    let private = parse_runner(Runner::Check, &["shell.lua", "--private-bus"]).unwrap();
    assert!(private.private_bus && !private.no_dbus);
    let render = parse_runner(Runner::Render, &["shell.lua", "-o", "x.png"]).unwrap();
    assert!(!render.no_dbus, "render keeps the bus it was started with");
}
