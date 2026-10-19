//! `morf approve`: its arguments, its walk over a configuration's imported
//! modules and plugins, the answers it records, and its exit status.
//!
//! The fixture is a project: `config/shell.lua` beside a `library/` of its
//! own, whose modules and plugin are imported. Approvals go to the
//! fixture's own file.

use std::fs;
use std::path::{Path, PathBuf};

use crate::approve::{Outcome, Request, approve, exit_code, parse};
use crate::config::{Command, parse_command};

struct Project {
    root: PathBuf,
}

impl Project {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("morf-approve-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let project = Self { root };
        let asker = project.mark("asker");
        let plugin = project.mark("plugin");
        project.write(
            "config/shell.lua",
            "local a = require('lib.fixture.asker')\n\
             local q = require('lib.fixture.quiet')\n\
             if false then require('lib.fixture.lazy') end\n\
             ASKED = a.child",
        );
        project.write(
            "library/lib/fixture/asker.lua",
            &format!(
                "local child = morf.spawn {{ command = {{ 'touch', '{}' }} }}\n\
                 pcall(morf.dbus.list_names, 'session')\n\
                 return {{ child = child }}",
                asker.display()
            ),
        );
        project.write(
            "library/lib/fixture/asker.permissions.lua",
            "return { run = { 'touch' } }",
        );
        project.write("library/lib/fixture/quiet.lua", "return {}");
        project.write("library/lib/fixture/quiet.permissions.lua", "return {}");
        project.write("library/lib/fixture/lazy.lua", "return {}");
        project.write(
            "library/plugin/p.lua",
            &format!(
                "morf.spawn {{ command = {{ 'touch', '{}' }} }}",
                plugin.display()
            ),
        );
        project
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn mark(&self, name: &str) -> PathBuf {
        self.root.join(format!("{name}.ran"))
    }

    fn config(&self) -> PathBuf {
        self.root.join("config/shell.lua")
    }

    fn store(&self) -> PathBuf {
        self.root.join("state/approvals.json")
    }

    fn approve(&self, answers: &str, yes: bool) -> (Result<Outcome, String>, String) {
        let request = Request {
            config: self.config(),
            yes,
        };
        let mut out = Vec::new();
        let result = approve(
            &request,
            Some(&self.store()),
            &mut answers.as_bytes(),
            &mut out,
        );
        (result, String::from_utf8(out).unwrap())
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn args(words: &[&str]) -> Vec<std::ffi::OsString> {
    words.iter().map(|word| word.into()).collect()
}

#[test]
fn approve_reads_its_arguments_and_never_runs_a_file_named_approve() {
    let parsed = |words: &[&str]| parse_command(&args(words));
    assert!(
        parsed(&["approve"])
            .unwrap_err()
            .contains("wants a configuration")
    );
    assert!(
        parsed(&["approve", "--yes"])
            .unwrap_err()
            .contains("wants a configuration")
    );
    assert!(
        parsed(&["approve", "a.lua", "b.lua"])
            .unwrap_err()
            .contains("one configuration")
    );
    assert!(
        parsed(&["approve", "--bogus", "a.lua"])
            .unwrap_err()
            .contains("--bogus")
    );
    for words in [
        &["approve", "--yes", "a.lua"][..],
        &["approve", "a.lua", "-y"],
    ] {
        match parsed(words).unwrap() {
            Command::Approve(request) => {
                assert_eq!(
                    request,
                    Request {
                        config: PathBuf::from("a.lua"),
                        yes: true
                    }
                );
            }
            _ => panic!("{words:?} is an approval"),
        }
    }
    match parsed(&["approve", "a.lua"]).unwrap() {
        Command::Approve(request) => assert!(!request.yes),
        _ => panic!("an approval"),
    }
    assert_eq!(
        parse(&["-c", "eva"]).unwrap().config.file_name().unwrap(),
        "init.lua"
    );
}

#[test]
fn the_exit_status_tells_approved_from_denied_from_broken() {
    assert_eq!(exit_code(&Ok(Outcome::Approved)), 0);
    assert_eq!(exit_code(&Ok(Outcome::Denied)), 1);
    assert_eq!(exit_code(&Err("unreadable".into())), 2);
    let missing = Request {
        config: PathBuf::from("/nonexistent/morf/shell.lua"),
        yes: true,
    };
    let result = approve(&missing, None, &mut "".as_bytes(), &mut Vec::new());
    assert_eq!(exit_code(&result), 2);
    let broken = Project::new("broken");
    fs::remove_dir_all(broken.root.join("library/plugin")).unwrap();
    broken.write("config/shell.lua", "this is not lua (");
    let (result, _) = broken.approve("", true);
    assert_eq!(exit_code(&result), 2, "a configuration that does not load");
}

#[test]
fn the_walk_asks_per_module_and_records_the_answers() {
    let project = Project::new("walk");
    // asker yes, lazy no, quiet is asked too (approving covers it), plugin yes.
    let (result, out) = project.approve("y\nn\ny\ny\n", false);
    assert_eq!(result, Ok(Outcome::Denied), "{out}");
    for expected in [
        "module lib.fixture.asker",
        "may run touch",
        "module lib.fixture.lazy",
        "(found in a source, not loaded at start)",
        "module lib.fixture.quiet",
        "asks for nothing",
        "plugin plugin/p.lua",
        "declares nothing",
        "3 approved, 1 not approved",
        "computed name",
    ] {
        assert!(out.contains(expected), "missing `{expected}` in:\n{out}");
    }
    assert!(
        !out.contains("runs anything"),
        "touch runs nothing else:\n{out}"
    );
    assert!(
        !project.mark("asker").exists(),
        "nothing ran privileged before the answers"
    );
    assert!(!project.mark("plugin").exists(), "not even the plugin");
    let store = fs::read_to_string(project.store()).unwrap();
    assert!(
        store.contains("asker.lua") && store.contains("p.lua"),
        "{store}"
    );
    assert!(!store.contains("lazy.lua"), "{store}");

    // Asked again: only what is not approved yet.
    let (result, out) = project.approve("y\n", false);
    assert_eq!(result, Ok(Outcome::Approved), "{out}");
    assert_eq!(out.matches("already approved").count(), 3, "{out}");
    assert_eq!(out.matches("approve? [y/N]").count(), 1, "{out}");
}

#[test]
fn yes_approves_everything_without_asking() {
    let project = Project::new("yes");
    let (result, out) = project.approve("", true);
    assert_eq!(result, Ok(Outcome::Approved), "{out}");
    assert!(!out.contains("approve? [y/N]"), "{out}");
    assert!(out.contains("4 approved, 0 not approved"), "{out}");
}

#[test]
fn an_approval_is_read_by_the_shell_it_was_given_for() {
    let project = Project::new("shell-reads");
    project.approve("", true).0.unwrap();
    let mut runtime = morf_lua::Runtime::default();
    runtime.set_approvals_path(project.store());
    let source = fs::read(project.config()).unwrap();
    morf_host::process::supervisor::execute_config(
        &mut runtime,
        &project.config(),
        &source,
        crate::config::LoadPolicy::default(),
    )
    .unwrap();
    assert!(
        appears(&project.mark("asker")),
        "the approved module ran free"
    );
    assert!(
        appears(&project.mark("plugin")),
        "the approved plugin ran free"
    );

    // Editing its manifest asks again.
    project.write(
        "library/lib/fixture/asker.permissions.lua",
        "return { run = true }",
    );
    let (result, out) = project.approve("n\n", false);
    assert_eq!(result, Ok(Outcome::Denied), "{out}");
    assert!(out.contains("changed since it was approved"), "{out}");
    assert!(
        out.contains("runs anything it is told: this asks for everything."),
        "{out}"
    );
}

#[test]
fn every_spelling_of_a_configuration_shares_one_store() {
    let project = Project::new("spellings");
    let link = project.root.join("linked");
    std::os::unix::fs::symlink(project.root.join("config"), &link).unwrap();
    // The shell's own folder under the state directory, which other tests
    // in this process may move.
    let store = |path: &Path| {
        let (runtime, _) = morf_host::headless::discover::discover(path, None);
        let store = runtime.approvals_path().unwrap();
        store.parent().unwrap().file_name().unwrap().to_owned()
    };
    let canonical = store(&project.config());
    assert_eq!(
        store(&link.join("shell.lua")),
        canonical,
        "through a symlinked folder"
    );
    assert_eq!(
        store(&project.root.join("config/../config/shell.lua")),
        canonical,
        "through a dotted path"
    );
}

fn appears(path: &Path) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !path.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    path.exists()
}
