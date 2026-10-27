//! Imported Lua modules: where each one came from, what it declares it
//! will touch, the approvals a person gave it, and the gate that holds an
//! unapproved one back while it loads.
//!
//! A module is config-local when it lives in the configuration's own
//! directory tree, first-party when it is the engine's own library (or a
//! byte-identical copy of one of its files), and imported otherwise: the
//! user's site folder, `MORF_RUNTIME_PATH` roots, plugin scripts outside
//! the configuration, and the library's `lib/integrations` and
//! `lib/compositor`, which morf ships but treats as third-party-shaped code.
//!
//! Enforcement is load-time only. While an unapproved imported module's top
//! level runs, every gated API called -- by it or by anything it causes to
//! run, a module it requires first included -- refuses with `nil` and a
//! reason, logged once. After its top level returns nothing it left behind
//! is gated: handlers, closures and the natives it hands out run as freely
//! as the configuration's own code.

mod gate;
mod loading;
mod manifest;
mod origin;
mod services;
mod store;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) use gate::{Gate, GateMode, Holder, gate, gated, gated_write, refused, refused_write};
pub use loading::APPROVALS_FILE;
pub(crate) use loading::Permissions;
pub use manifest::manifest_path;
pub(crate) use manifest::{read_manifest, run_data};
pub(crate) use origin::INTEGRATIONS;
pub use origin::{Origin, Origins, canonical, engine_library};
pub(crate) use services::read_catalog;
pub use services::{SERVICE_CATALOG, ServiceEntry, unknown_service};
pub use store::{Approval, Approvals, ModuleTree, fingerprint};

/// The groups a declaration may name. The gated ones are enforced at load;
/// the rest are declared for the person approving and for the service
/// registry, which names the services a shell binds under `services`.
pub const PERMISSION_GROUPS: &[&str] = &[
    "run", "process", "fs", "dbus", "http", "socket", "pam", "greetd", "services", "system",
    "desktop", "media", "io",
];

/// The groups the load-time gate enforces, each named by the APIs it covers.
pub const GATED_GROUPS: &[&str] = &["run", "fs", "dbus", "http", "socket", "pam", "greetd"];

/// What a declaration grants within one group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Grant {
    /// `group = true`: anything in the group.
    Any,
    /// `group = { "name", ... }`: these programs, hosts, buses or services.
    Names(Vec<String>),
}

/// A manifest read as data: group name to what it grants.
pub type Declaration = BTreeMap<String, Grant>;

/// Whether a record is a `require`d module or a plugin script.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    Module,
    Plugin,
}

/// One module a configuration reaches: where it came from and what it
/// declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleDeclaration {
    /// The `require` name, or a plugin script's path under its root.
    pub name: String,
    /// The entry file, canonical.
    pub path: PathBuf,
    pub origin: Origin,
    pub kind: ModuleKind,
    /// The manifest file, when one sits beside the entry.
    pub manifest: Option<PathBuf>,
    /// What the manifest grants; empty when there is none.
    pub grants: Declaration,
    /// Why the manifest could not be read as a declaration.
    pub problem: Option<String>,
    /// Whether it ran in this load, rather than being found in a source.
    pub loaded: bool,
}

impl ModuleDeclaration {
    /// A record with nothing declared yet.
    pub fn new(name: impl Into<String>, path: PathBuf, origin: Origin, kind: ModuleKind) -> Self {
        Self {
            name: name.into(),
            path,
            origin,
            kind,
            manifest: None,
            grants: Declaration::new(),
            problem: None,
            loaded: true,
        }
    }

    pub fn imported(&self) -> bool {
        self.origin == Origin::Imported
    }

    pub fn declared(&self) -> bool {
        self.manifest.is_some()
    }

    /// Whether the module is one of the library's integrations.
    pub fn is_integration(&self) -> bool {
        self.kind == ModuleKind::Module
            && INTEGRATIONS
                .iter()
                .any(|prefix| self.name.starts_with(prefix))
    }

    /// Declared groups outside the known vocabulary.
    pub fn unknown_groups(&self) -> Vec<&str> {
        self.grants
            .keys()
            .map(String::as_str)
            .filter(|group| !PERMISSION_GROUPS.contains(group))
            .collect()
    }

    /// The files whose bytes the approval covers.
    pub fn tree(&self) -> ModuleTree {
        ModuleTree::of(&self.path, self.manifest.as_deref())
    }

    /// The directory a module may write in while it loads: a directory
    /// module's own folder. A single-file module or a plugin has none.
    pub fn own_directory(&self) -> Option<&Path> {
        (self.kind == ModuleKind::Module
            && self.path.file_name().is_some_and(|name| name == "init.lua"))
        .then(|| self.path.parent())
        .flatten()
    }

    /// The declaration as one stable line, for the fingerprint.
    pub fn declaration_text(&self) -> String {
        self.grants
            .iter()
            .map(|(group, grant)| match grant {
                Grant::Any => format!("{group}=*"),
                Grant::Names(names) => format!("{group}={}", names.join(",")),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
