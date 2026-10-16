//! What an imported module says it intends to touch (PLAN item 4).
//!
//! A module may ship `permissions.lua` beside its entry point, returning
//! a table whose keys are permission groups. The loader records each
//! loaded module's origin -- config-local (resolved under the
//! configuration's own root) or imported (every other root, plugin
//! roots included) -- and its declaration; `morf check` prints the
//! declarations of imported modules.
//!
//! Enforcement is the stop condition's fallback, load-time only: while
//! an imported module with no current approval runs its top level,
//! gated APIs (`morf.run`/`morf.spawn`, `morf.connect`) refuse with
//! `nil` and a reason. Closures carry no origin, so a handler an
//! unapproved module registers is not gated after load -- that boundary
//! is stated, not hidden. The group names are shared with the service
//! registry (`library/lib/services/registry.lua`): a service a shell
//! binds is a capability it uses, named the same in both places.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The permission groups a declaration may name, mirroring the public
/// API groups under `crates/engine/morf-lua/src/api/`.
pub const PERMISSION_GROUPS: &[&str] = &[
    "run", "process", "fs", "dbus", "http", "socket", "services", "pam", "system", "desktop",
    "media", "io",
];

/// One loaded module: where it came from and what it declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleDeclaration {
    /// The `require` name the module was loaded under.
    pub name: String,
    /// The file the module was loaded from.
    pub path: PathBuf,
    /// Loaded from outside the configuration's own root.
    pub imported: bool,
    /// Whether a `permissions.lua` sat beside the entry point.
    pub declared: bool,
    /// The declaration's groups, sorted; empty when undeclared.
    pub groups: Vec<String>,
}

impl ModuleDeclaration {
    /// Declaration groups outside the known vocabulary.
    pub fn unknown_groups(&self) -> Vec<&str> {
        self.groups
            .iter()
            .map(String::as_str)
            .filter(|group| !PERMISSION_GROUPS.contains(group))
            .collect()
    }
}

/// FNV-1a, as the shader compiler hashes its emitted source.
fn fnv1a(bytes: &[u8], value: &mut u64) {
    for byte in bytes {
        *value ^= u64::from(*byte);
        *value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

/// A module's fingerprint: FNV-1a over its tree's bytes (every regular
/// file under the entry point's directory, in sorted relative-path
/// order) plus its declaration groups. Any change to code or
/// declaration changes the fingerprint, which is what lapses an
/// approval (step 2).
pub fn fingerprint(record: &ModuleDeclaration) -> Result<String, String> {
    let directory = record
        .path
        .parent()
        .ok_or_else(|| format!("module {} has no directory", record.path.display()))?;
    let mut files = Vec::new();
    collect_files(directory, directory, &mut files)?;
    files.sort();
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for relative in &files {
        fnv1a(relative.as_bytes(), &mut value);
        fnv1a(&[0xff], &mut value);
        let bytes = std::fs::read(directory.join(relative))
            .map_err(|error| format!("{}: {error}", directory.join(relative).display()))?;
        fnv1a(&bytes, &mut value);
        fnv1a(&[0xff], &mut value);
    }
    fnv1a(record.groups.join("\n").as_bytes(), &mut value);
    Ok(format!("{value:016x}"))
}

fn collect_files(root: &Path, directory: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{error}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else if path.is_file() {
            out.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

/// Approvals on disk: canonical module path -> fingerprint, as JSON.
/// An absent file is an empty store; a fingerprint mismatch is a
/// lapsed approval, never an error.
#[derive(Debug, Default)]
pub struct Approvals {
    entries: BTreeMap<String, String>,
}

impl Approvals {
    pub fn load(path: &Path) -> Result<Self, String> {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Ok(Self::default());
        };
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        let mut entries = BTreeMap::new();
        if let Some(object) = value.get("approvals").and_then(|v| v.as_object()) {
            for (module, fingerprint) in object {
                if let Some(fingerprint) = fingerprint.as_str() {
                    entries.insert(module.clone(), fingerprint.to_owned());
                }
            }
        }
        Ok(Self { entries })
    }

    pub fn approve(&mut self, record: &ModuleDeclaration) -> Result<String, String> {
        let fingerprint = fingerprint(record)?;
        let key = canonical_key(&record.path);
        self.entries.insert(key, fingerprint.clone());
        Ok(fingerprint)
    }

    pub fn is_approved(&self, record: &ModuleDeclaration) -> bool {
        let Ok(current) = fingerprint(record) else {
            return false;
        };
        self.entries.get(&canonical_key(&record.path)) == Some(&current)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        let approvals: serde_json::Map<String, serde_json::Value> = self
            .entries
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        let text = serde_json::to_string_pretty(&serde_json::json!({
            "version": 1,
            "approvals": approvals,
        }))
        .map_err(|error| error.to_string())?;
        std::fs::write(path, text + "\n").map_err(|error| format!("{}: {error}", path.display()))
    }
}

fn canonical_key(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}
