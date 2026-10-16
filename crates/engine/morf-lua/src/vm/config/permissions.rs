//! The `Runtime`'s side of imported-module permissions: the configuration
//! it runs, where approvals are kept, plugin scripts, and what it loaded.

use std::path::{Path, PathBuf};

use crate::vm::loader::resolve_runtime_module;
use crate::vm::permissions::{GateMode, ModuleDeclaration, ModuleKind, canonical};
use crate::vm::types::{Error, Runtime};

/// How the load-time gate treats imported modules.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PermissionMode {
    /// Unapproved imported modules are held back while they load.
    #[default]
    Enforce,
    /// Every imported module is held back, approved or not.
    HoldImported,
    /// Every gated call is refused, whoever makes it.
    RefuseAll,
}

impl Runtime {
    pub fn set_permission_mode(&mut self, mode: PermissionMode) {
        let mode = match mode {
            PermissionMode::Enforce => GateMode::Enforce,
            PermissionMode::HoldImported => GateMode::ImportedUnapproved,
            PermissionMode::RefuseAll => GateMode::RefuseAll,
        };
        self.reactive.borrow_mut().permissions_mut().set_mode(mode);
    }

    /// The configuration whose folder is config-local.
    pub fn set_config_file(&mut self, path: &Path) {
        self.permissions
            .borrow_mut()
            .set_config_file(path.to_path_buf());
    }

    pub fn config_file(&self) -> Option<PathBuf> {
        self.permissions
            .borrow()
            .config_file()
            .map(Path::to_path_buf)
    }

    /// The first chunk run from a file names the configuration, unless one
    /// was named already.
    pub(crate) fn adopt_config_file(&mut self, name: &str) {
        let path = Path::new(name);
        let is_file = !name.starts_with(['=', '@'])
            && path
                .parent()
                .is_some_and(|parent| !parent.as_os_str().is_empty());
        if is_file && self.permissions.borrow().config_file().is_none() {
            self.set_config_file(path);
        }
    }

    /// The engine library whose files are first-party; by default the one
    /// installed beside the executable.
    pub fn set_engine_library(&mut self, library: Option<PathBuf>) {
        self.permissions.borrow_mut().set_engine_library(library);
    }

    /// Keeps approvals at `path` instead of the shell's state directory.
    pub fn set_approvals_path(&mut self, path: PathBuf) {
        self.permissions.borrow_mut().set_store_path(path);
    }

    /// Where this shell's approvals live: JSON under its XDG state directory.
    pub fn approvals_path(&self) -> Result<PathBuf, String> {
        let root = self.reactive.borrow().shell_root().to_path_buf();
        self.permissions.borrow().store_path(&root)
    }

    /// Reads the approvals again at the next module, after they changed.
    pub fn reload_approvals(&mut self) {
        self.permissions.borrow_mut().reload_approvals();
    }

    /// Every module and plugin this runtime met, by name.
    pub fn module_declarations(&self) -> Vec<ModuleDeclaration> {
        let mut records = self.permissions.borrow().records().to_vec();
        records.sort_by(|a, b| a.name.cmp(&b.name));
        records
    }

    /// What `require(name)` would load and declare, without running it.
    pub fn declare_module(&mut self, name: &str) -> Option<ModuleDeclaration> {
        let roots = self.module_roots.borrow().clone();
        let (path, _) = resolve_runtime_module(&roots, name).ok()?;
        let limits = self.limits;
        let permissions = &self.permissions;
        let (mut record, _) = self.lua.enter(|ctx| {
            permissions
                .borrow_mut()
                .declare(ctx, &roots, name, &path, ModuleKind::Module, limits)
        });
        record.loaded = false;
        Some(record)
    }

    /// Runs a plugin script found on the runtime path, as imported code
    /// when it is outside the configuration.
    pub fn execute_plugin(&mut self, path: &Path, source: &[u8]) -> Result<(), Error> {
        let resolved = canonical(path);
        let name = self
            .module_roots
            .borrow()
            .iter()
            .map(|root| canonical(root))
            .filter(|root| resolved.starts_with(root))
            .max_by_key(|root| root.components().count())
            .and_then(|root| resolved.strip_prefix(root).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| resolved.clone());
        let name = name.to_string_lossy().into_owned();
        self.execute_scoped(&path.to_string_lossy(), source, Some((&name, &resolved)))
    }
}
