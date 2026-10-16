//! Approvals: what a person approved, keyed by the module's canonical path
//! and pinned to a fingerprint of its code and declaration.
//!
//! The store is JSON under the shell's XDG state directory. A store with
//! no entry for a module answers without reading the module at all; an
//! entry whose fingerprint no longer matches is a lapsed approval.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::ModuleDeclaration;

/// The files an approval covers: a directory module's whole folder, or a
/// single file and its manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleTree {
    pub base: PathBuf,
    pub files: Vec<PathBuf>,
}

impl ModuleTree {
    pub fn of(entry: &Path, manifest: Option<&Path>) -> Self {
        let base = entry
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        let mut files = Vec::new();
        if entry.file_name().is_some_and(|name| name == "init.lua") {
            collect_files(&base, &mut files);
        } else {
            files.push(entry.to_path_buf());
            files.extend(
                manifest
                    .filter(|path| path.is_file())
                    .map(Path::to_path_buf),
            );
        }
        files.sort();
        files.dedup();
        Self { base, files }
    }
}

fn collect_files(directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

/// FNV-1a, as the shader compiler hashes its emitted source.
fn fnv1a(bytes: &[u8], value: &mut u64) {
    for byte in bytes {
        *value ^= u64::from(*byte);
        *value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

/// FNV-1a over the module tree -- each file's path under the tree's base
/// and its bytes -- and the declaration it was read with.
pub fn fingerprint(record: &ModuleDeclaration) -> Result<String, String> {
    let tree = record.tree();
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for file in &tree.files {
        let relative = file.strip_prefix(&tree.base).unwrap_or(file);
        fnv1a(relative.to_string_lossy().as_bytes(), &mut value);
        fnv1a(&[0xff], &mut value);
        let bytes = std::fs::read(file).map_err(|error| format!("{}: {error}", file.display()))?;
        fnv1a(&bytes, &mut value);
        fnv1a(&[0xff], &mut value);
    }
    fnv1a(record.declaration_text().as_bytes(), &mut value);
    Ok(format!("{value:016x}"))
}

/// Where a module stands with the store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Approval {
    /// Approved, and unchanged since.
    Approved,
    /// Approved once; its code or declaration has changed since.
    Lapsed,
    /// Never approved.
    Unknown,
}

/// The approvals on disk: canonical module path to fingerprint.
#[derive(Debug, Default, Clone)]
pub struct Approvals {
    entries: BTreeMap<String, String>,
}

impl Approvals {
    /// The store at `path`; an absent file is an empty store.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(format!("{}: {error}", path.display())),
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

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Looks the module up first and hashes it only when there is an entry
    /// to compare with.
    pub fn status(&self, record: &ModuleDeclaration) -> Approval {
        let Some(stored) = self.entries.get(&key(&record.path)) else {
            return Approval::Unknown;
        };
        match fingerprint(record) {
            Ok(current) if &current == stored => Approval::Approved,
            _ => Approval::Lapsed,
        }
    }

    pub fn is_approved(&self, record: &ModuleDeclaration) -> bool {
        self.status(record) == Approval::Approved
    }

    /// Records `record` as approved as it stands now.
    pub fn approve(&mut self, record: &ModuleDeclaration) -> Result<String, String> {
        let fingerprint = fingerprint(record)?;
        self.entries.insert(key(&record.path), fingerprint.clone());
        Ok(fingerprint)
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

fn key(path: &Path) -> String {
    super::canonical(path).to_string_lossy().into_owned()
}
