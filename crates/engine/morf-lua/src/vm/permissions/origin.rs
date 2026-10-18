//! Where a module comes from: the configuration's own tree, the engine's
//! own library, or somewhere imported.
//!
//! The configuration's folder is canonical, paths are absolute with `..`
//! resolved, and the most specific root wins: a configuration at
//! `~/shell.lua` does not own `~/.local/share/morf/site`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// The module-name prefix of the library's integrations, which are imported
/// wherever they are installed.
pub(crate) const INTEGRATIONS: &str = "lib.integrations.";

/// The integrations' folder under a library root.
const INTEGRATIONS_DIR: &str = "lib/integrations";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// In the configuration's own directory tree.
    ConfigLocal,
    /// The engine's own library, as it shipped.
    FirstParty,
    /// Anything else.
    Imported,
}

/// `path` as the filesystem resolves it: the file itself when it exists,
/// else its folder resolved with the name joined on, else made absolute.
pub fn canonical(path: &Path) -> PathBuf {
    if let Ok(resolved) = std::fs::canonicalize(path) {
        return resolved;
    }
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    match (absolute.parent(), absolute.file_name()) {
        (Some(parent), Some(name)) => std::fs::canonicalize(parent)
            .map(|parent| parent.join(name))
            .unwrap_or(absolute),
        _ => absolute,
    }
}

/// The library installed with the running executable: `../share/morf/library`
/// beside an installed binary, or the checkout's `library/` above a cargo
/// build.
pub fn engine_library() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let executable = std::fs::canonicalize(&executable).unwrap_or(executable);
    let folder = executable.parent()?;
    [
        folder.join("../share/morf/library"),
        folder.join("../../library"),
        folder.join("../../../library"),
    ]
    .into_iter()
    .find(|candidate| candidate.join("lib").is_dir())
    .and_then(|found| std::fs::canonicalize(found).ok())
}

/// `path` made absolute with `.` and `..` taken out, symlinks left as they
/// are: the path as `require` reached it.
pub fn lexical(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Classifies module paths against one configuration's roots.
#[derive(Debug, Default)]
pub struct Origins {
    config_dir: Option<PathBuf>,
    library: Option<PathBuf>,
    roots: Vec<PathBuf>,
    copies: RefCell<HashMap<PathBuf, bool>>,
}

impl Origins {
    /// `config_dir` is the configuration's folder, `library` the engine's
    /// library and `roots` every folder `require` searches.
    pub fn new(config_dir: Option<&Path>, library: Option<&Path>, roots: &[PathBuf]) -> Self {
        Self {
            config_dir: config_dir.map(lexical),
            library: library.map(canonical),
            roots: roots.iter().map(|root| lexical(root)).collect(),
            copies: RefCell::default(),
        }
    }

    /// The canonical path of `path` and where it comes from.
    ///
    /// The root is the most specific one holding the path as `require`
    /// reached it, so a symlink the configuration keeps in its own folder is
    /// its own. The engine's library is recognised by where the file really
    /// is, however it was reached.
    pub fn classify(&self, path: &Path) -> (PathBuf, Origin) {
        let reached = lexical(path);
        let real = canonical(path);
        let origin = self.origin_of(&reached, &real);
        (real, origin)
    }

    fn origin_of(&self, reached: &Path, real: &Path) -> Origin {
        let config = self.config_dir.iter().map(|dir| (dir, true));
        let others = self.roots.iter().map(|dir| (dir, false));
        let mut best: Option<(&PathBuf, bool)> = None;
        for (dir, is_config) in config.chain(others) {
            let longer = best
                .is_none_or(|(current, _)| dir.components().count() > current.components().count());
            if reached.starts_with(dir) && longer {
                best = Some((dir, is_config));
            }
        }
        if let Some((_, true)) = best {
            return Origin::ConfigLocal;
        }
        if let Some(library) = &self.library
            && let Ok(relative) = real.strip_prefix(library)
        {
            return match relative.starts_with(INTEGRATIONS_DIR) {
                true => Origin::Imported,
                false => Origin::FirstParty,
            };
        }
        let Some((root, _)) = best else {
            return Origin::Imported;
        };
        let relative = reached.strip_prefix(root).unwrap_or(reached);
        if !relative.starts_with(INTEGRATIONS_DIR) && self.is_library_copy(real, relative) {
            Origin::FirstParty
        } else {
            Origin::Imported
        }
    }

    /// Whether `path` is byte for byte the engine library's file at
    /// `relative`.
    fn is_library_copy(&self, path: &Path, relative: &Path) -> bool {
        let Some(library) = &self.library else {
            return false;
        };
        if let Some(known) = self.copies.borrow().get(path) {
            return *known;
        }
        let shipped = library.join(relative);
        let same = match (std::fs::read(path), std::fs::read(&shipped)) {
            (Ok(ours), Ok(theirs)) => ours == theirs,
            _ => false,
        };
        self.copies.borrow_mut().insert(path.to_path_buf(), same);
        same
    }
}
