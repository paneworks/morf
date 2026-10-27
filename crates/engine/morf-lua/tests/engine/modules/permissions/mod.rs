//! Imported modules: where each comes from, what it declares, what the
//! approvals say, and the gate that holds an unapproved one while it loads.
//!
//! Every fixture is a scratch tree with a configuration folder, a site
//! folder of imported modules and an engine library, and keeps its
//! approvals in its own file: nothing here reads or writes the real XDG
//! state directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use morf_lua::{ModuleDeclaration, Runtime};

mod gate;
mod gate_load;
mod manifest;
mod origin;
mod store;

pub(super) struct Fixture {
    pub(super) root: PathBuf,
}

impl Fixture {
    pub(super) fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("morf-perm-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for folder in ["config", "site", "library/lib", "state", "out"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        let root = fs::canonicalize(root).unwrap();
        Self { root }
    }

    pub(super) fn config(&self) -> PathBuf {
        self.root.join("config")
    }

    pub(super) fn site(&self) -> PathBuf {
        self.root.join("site")
    }

    pub(super) fn library(&self) -> PathBuf {
        self.root.join("library")
    }

    pub(super) fn store(&self) -> PathBuf {
        self.root.join("state/approvals.json")
    }

    /// A file outside every module, for writes and spawn markers.
    pub(super) fn out(&self, name: &str) -> PathBuf {
        self.root.join("out").join(name)
    }

    pub(super) fn write(&self, relative: &str, text: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }

    /// A runtime for `config/shell.lua` with `site` imported and `library`
    /// the engine's own.
    pub(super) fn runtime(&self) -> Runtime {
        let mut runtime = Runtime::default();
        runtime.set_engine_library(Some(self.library()));
        runtime.set_approvals_path(self.store());
        runtime.set_module_roots(vec![self.config(), self.library(), self.site()]);
        runtime.set_config_file(&self.config().join("shell.lua"));
        runtime.set_shell_root(self.config());
        runtime
    }

    pub(super) fn run(&self, runtime: &mut Runtime, source: &str) {
        let shell = self.config().join("shell.lua");
        runtime
            .execute(&shell.to_string_lossy(), source.as_bytes())
            .unwrap_or_else(|error| panic!("{error}"));
    }

    pub(super) fn record(&self, runtime: &Runtime, name: &str) -> ModuleDeclaration {
        runtime
            .module_declarations()
            .into_iter()
            .find(|record| record.name == name)
            .unwrap_or_else(|| panic!("{name} was not recorded"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Lua source for a harmless spawn: `touch` the marker.
pub(super) fn touch(marker: &Path) -> String {
    format!(
        r#"morf.process.spawn {{ command = {{ "touch", "{}" }} }}"#,
        marker.display()
    )
}

/// Whether `path` appears within two seconds.
pub(super) fn appears(path: &Path) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// Whether `path` is still absent a moment after a spawn would have made it.
pub(super) fn stays_absent(path: &Path) -> bool {
    std::thread::sleep(Duration::from_millis(300));
    !path.exists()
}

/// The permission lines the runtime logged.
pub(super) fn refusals(runtime: &mut Runtime) -> Vec<String> {
    runtime
        .take_logs()
        .into_iter()
        .map(|entry| entry.message)
        .filter(|message| message.starts_with("permission:"))
        .collect()
}
