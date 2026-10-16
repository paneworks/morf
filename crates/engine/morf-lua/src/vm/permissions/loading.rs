//! What one runtime knows about the modules it loads: the configuration it
//! runs, the store it reads approvals from (once), the manifests it has read
//! (once each), and a record of every module and plugin it met.

use luna::Context;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::{
    Approvals, Declaration, GateMode, Holder, ModuleDeclaration, ModuleKind, Origin, Origins,
    engine_library, manifest_path, read_manifest,
};
use crate::LogLevel;
use crate::runtime::state::{ReactiveState, StateCell};
use crate::vm::types::{Limits, StorageKind, shell_storage_dir};

/// The file approvals are kept in, under the shell's state directory.
pub const APPROVALS_FILE: &str = "permissions-approvals.json";

#[derive(Default)]
pub(crate) struct Permissions {
    config_file: Option<PathBuf>,
    library: Option<Option<PathBuf>>,
    store_path: Option<PathBuf>,
    approvals: Option<Approvals>,
    manifests: HashMap<PathBuf, Result<Declaration, String>>,
    records: Vec<ModuleDeclaration>,
    origins: Option<(Vec<PathBuf>, Origins)>,
}

impl Permissions {
    pub(crate) fn config_file(&self) -> Option<&Path> {
        self.config_file.as_deref()
    }

    pub(crate) fn set_config_file(&mut self, path: PathBuf) {
        self.config_file = Some(super::canonical(&path));
        self.origins = None;
    }

    pub(crate) fn set_engine_library(&mut self, library: Option<PathBuf>) {
        self.library = Some(library);
        self.origins = None;
    }

    pub(crate) fn set_store_path(&mut self, path: PathBuf) {
        self.store_path = Some(path);
        self.approvals = None;
    }

    /// Forgets the approvals read, so the next module reads the store again.
    pub(crate) fn reload_approvals(&mut self) {
        self.approvals = None;
    }

    pub(crate) fn records(&self) -> &[ModuleDeclaration] {
        &self.records
    }

    /// Where approvals live: the path set for this runtime, else the shell's
    /// state directory.
    pub(crate) fn store_path(&self, shell_root: &Path) -> Result<PathBuf, String> {
        if let Some(path) = &self.store_path {
            return Ok(path.clone());
        }
        let root = super::canonical(shell_root);
        Ok(shell_storage_dir(&root, StorageKind::State)?.join(APPROVALS_FILE))
    }

    fn origins(&mut self, roots: &[PathBuf]) -> &Origins {
        let stale = self
            .origins
            .as_ref()
            .is_none_or(|(seen, _)| seen.as_slice() != roots);
        if stale {
            let library = self.library.get_or_insert_with(engine_library).clone();
            let config_dir = self.config_file.as_deref().and_then(Path::parent);
            let origins = Origins::new(config_dir, library.as_deref(), roots);
            self.origins = Some((roots.to_vec(), origins));
        }
        &self.origins.as_ref().expect("origins were just built").1
    }

    /// Classifies `path` and reads its manifest when it is imported.
    pub(crate) fn declare<'gc>(
        &mut self,
        ctx: Context<'gc>,
        roots: &[PathBuf],
        name: &str,
        path: &Path,
        kind: ModuleKind,
        limits: Limits,
    ) -> (ModuleDeclaration, Option<String>) {
        let (path, origin) = self.origins(roots).classify(path);
        let mut record = ModuleDeclaration::new(name, path, origin, kind);
        if origin != Origin::Imported {
            return (record, None);
        }
        let manifest = manifest_path(&record.path);
        if !manifest.is_file() {
            return (record, None);
        }
        let first = !self.manifests.contains_key(&manifest);
        let read = self
            .manifests
            .entry(manifest.clone())
            .or_insert_with(|| read_manifest(ctx, &manifest, limits))
            .clone();
        record.manifest = Some(manifest);
        let warning = match read {
            Ok(grants) => {
                record.grants = grants;
                None
            }
            Err(problem) => {
                record.problem = Some(problem.clone());
                first.then(|| format!("permission: {problem}"))
            }
        };
        (record, warning)
    }

    /// Whether the store holds a current approval for `record`.
    fn approved(
        &mut self,
        record: &ModuleDeclaration,
        shell_root: &Path,
    ) -> (bool, Option<String>) {
        let mut warning = None;
        if self.approvals.is_none() {
            let loaded = self
                .store_path(shell_root)
                .and_then(|path| Approvals::load(&path));
            self.approvals = Some(loaded.unwrap_or_else(|error| {
                warning = Some(format!(
                    "permission: approvals unreadable, none apply: {error}"
                ));
                Approvals::default()
            }));
        }
        let approvals = self.approvals.as_ref().expect("approvals were just read");
        (approvals.is_approved(record), warning)
    }

    /// Records a module or plugin about to run its top level, and decides
    /// whether the gate holds it back.
    pub(crate) fn admit<'gc>(
        &mut self,
        ctx: Context<'gc>,
        state: &Rc<StateCell<ReactiveState>>,
        roots: &[PathBuf],
        name: &str,
        path: &Path,
        kind: ModuleKind,
    ) -> Option<Rc<Holder>> {
        let limits = *state.borrow().limits();
        let (record, mut warnings) = {
            let (record, warning) = self.declare(ctx, roots, name, path, kind, limits);
            (record, warning.into_iter().collect::<Vec<_>>())
        };
        let mode = state.borrow().permissions().mode();
        let held = record.imported()
            && match mode {
                GateMode::Enforce => {
                    let shell_root = state.borrow().shell_root().to_path_buf();
                    let (approved, warning) = self.approved(&record, &shell_root);
                    warnings.extend(warning);
                    !approved
                }
                GateMode::ImportedUnapproved | GateMode::RefuseAll => true,
            };
        for warning in warnings {
            state.borrow_mut().log(LogLevel::Warn, warning);
        }
        let holder = held.then(|| {
            Rc::new(Holder {
                name: record.name.clone(),
                path: record.path.clone(),
                own_dir: record.own_directory().map(Path::to_path_buf),
            })
        });
        if !self.records.iter().any(|known| known.path == record.path) {
            self.records.push(record);
        }
        holder
    }
}
