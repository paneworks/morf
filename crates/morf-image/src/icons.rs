use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;

use crate::image_cache::ImageError;
use std::path::{Path, PathBuf};

/// XDG icon-theme resolver with inheritance and size matching.
#[derive(Clone, Debug)]
pub struct IconResolver {
    roots: Vec<PathBuf>,
    pixmaps: Vec<PathBuf>,
}

impl IconResolver {
    /// Creates a resolver over explicit icon-theme roots.
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self {
            roots,
            pixmaps: Vec::new(),
        }
    }

    /// Creates a resolver over icon-theme roots and the unthemed pixmap
    /// directories searched after every theme.
    pub fn with_pixmaps(roots: Vec<PathBuf>, pixmaps: Vec<PathBuf>) -> Self {
        Self { roots, pixmaps }
    }

    /// Creates a resolver from XDG data directories and the legacy user icon root.
    pub fn from_environment() -> Self {
        Self::from_directories(
            env::var_os("HOME").map(PathBuf::from),
            env::var_os("XDG_DATA_HOME").map(PathBuf::from),
            env::var_os("XDG_DATA_DIRS"),
        )
    }

    /// The Icon Theme spec's base directories, in its order: `~/.icons`,
    /// then `icons` under each data directory -- the ones desktop entries
    /// come from (`morf_desktop::xdg_data_dirs`): the user's data home,
    /// `XDG_DATA_DIRS`, then `/usr/local/share` and `/usr/share` whatever
    /// that list said, since a Nix shell's list names only the store. After
    /// the themes, `pixmaps` under each system data directory, which so
    /// always ends at `/usr/share/pixmaps`, the spec's last resort.
    pub fn from_directories(
        home: Option<PathBuf>,
        data_home: Option<PathBuf>,
        data_dirs: Option<std::ffi::OsString>,
    ) -> Self {
        let has_data_home = data_home
            .as_ref()
            .is_some_and(|path| !path.as_os_str().is_empty())
            || home.is_some();
        let data = morf_desktop::xdg_data_dirs_from(
            data_home.map(PathBuf::into_os_string),
            home.clone().map(PathBuf::into_os_string),
            data_dirs,
        );
        let mut roots = Vec::new();
        let mut pixmaps = Vec::new();
        if let Some(home) = &home {
            roots.push(home.join(".icons"));
        }
        for (index, root) in data.iter().enumerate() {
            roots.push(root.join("icons"));
            // The user's data home keeps no pixmaps.
            if !(has_data_home && index == 0) {
                pixmaps.push(root.join("pixmaps"));
            }
        }
        Self { roots, pixmaps }
    }

    /// Finds the closest icon file for a physical pixel size.
    ///
    /// In the Icon Theme spec's order: the theme and what it inherits, then
    /// `hicolor`, then an unthemed file directly in a base directory, then
    /// the pixmap directories. A name that is an absolute path to a file is
    /// that file, as a desktop entry's `Icon=` may be.
    pub fn find(&self, name: &str, theme: &str, size: u32) -> Result<PathBuf, ImageError> {
        let path = Path::new(name);
        if path.is_absolute() {
            return if path.is_file() {
                Ok(path.to_path_buf())
            } else {
                Err(ImageError::IconNotFound(name.to_owned()))
            };
        }
        if name.is_empty() || name.contains('/') {
            return Err(ImageError::IconNotFound(name.to_owned()));
        }
        let mut visited = HashSet::new();
        if let Some(path) = self.find_theme(name, theme, size, &mut visited) {
            return Ok(path);
        }
        if theme != "hicolor"
            && let Some(path) = self.find_theme(name, "hicolor", size, &mut visited)
        {
            return Ok(path);
        }
        for root in self.roots.iter().chain(&self.pixmaps) {
            if let Some(path) = find_named_file(root, name) {
                return Ok(path);
            }
        }
        Err(ImageError::IconNotFound(name.to_owned()))
    }

    fn find_theme(
        &self,
        name: &str,
        theme: &str,
        size: u32,
        visited: &mut HashSet<String>,
    ) -> Option<PathBuf> {
        if !visited.insert(theme.to_owned()) {
            return None;
        }
        let mut inherited = Vec::new();
        let mut candidates = Vec::new();
        for root in &self.roots {
            let theme_root = root.join(theme);
            let Some(index) = ThemeIndex::load(&theme_root.join("index.theme")) else {
                continue;
            };
            inherited.extend(index.inherits.iter().cloned());
            for directory in index.directories {
                if let Some(path) = find_named_file(&theme_root.join(&directory.name), name) {
                    candidates.push((directory.distance(size), path));
                }
            }
        }
        if let Some((_, path)) = candidates.into_iter().min_by_key(|(distance, _)| *distance) {
            return Some(path);
        }
        for parent in inherited {
            if let Some(path) = self.find_theme(name, &parent, size, visited) {
                return Some(path);
            }
        }
        None
    }
}

#[derive(Default)]
struct ThemeIndex {
    inherits: Vec<String>,
    directories: Vec<IconDirectory>,
}

impl ThemeIndex {
    fn load(path: &Path) -> Option<Self> {
        let source = fs::read_to_string(path).ok()?;
        let sections = parse_ini(&source);
        let theme = sections.get("Icon Theme")?;
        let inherits = split_list(theme.get("Inherits"));
        let names = split_list(theme.get("Directories"));
        let directories = names
            .into_iter()
            .map(|name| IconDirectory::from_section(name.clone(), sections.get(&name)))
            .collect();
        Some(Self {
            inherits,
            directories,
        })
    }
}

struct IconDirectory {
    name: String,
    size: u32,
    min_size: u32,
    max_size: u32,
    threshold: u32,
    kind: DirectoryType,
}

#[derive(Clone, Copy)]
enum DirectoryType {
    Fixed,
    Scalable,
    Threshold,
}

impl IconDirectory {
    fn from_section(name: String, section: Option<&HashMap<String, String>>) -> Self {
        let field = |key: &str| section.and_then(|values| values.get(key));
        let size = parse_u32(field("Size")).unwrap_or(48);
        let kind = match field("Type").map(String::as_str) {
            Some("Scalable") => DirectoryType::Scalable,
            Some("Threshold") => DirectoryType::Threshold,
            _ => DirectoryType::Fixed,
        };
        Self {
            name,
            size,
            min_size: parse_u32(field("MinSize")).unwrap_or(size),
            max_size: parse_u32(field("MaxSize")).unwrap_or(size),
            threshold: parse_u32(field("Threshold")).unwrap_or(2),
            kind,
        }
    }

    fn distance(&self, requested: u32) -> u32 {
        let (minimum, maximum) = match self.kind {
            DirectoryType::Fixed => (self.size, self.size),
            DirectoryType::Scalable => (self.min_size, self.max_size),
            DirectoryType::Threshold => (
                self.size.saturating_sub(self.threshold),
                self.size.saturating_add(self.threshold),
            ),
        };
        if requested < minimum {
            minimum - requested
        } else {
            requested.saturating_sub(maximum)
        }
    }
}

fn parse_ini(source: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections = HashMap::<String, HashMap<String, String>>::new();
    let mut current = String::new();
    for line in source.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            current = section.to_owned();
        } else if let Some((key, value)) = line.split_once('=') {
            sections
                .entry(current.clone())
                .or_default()
                .insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    sections
}

fn split_list(value: Option<&String>) -> Vec<String> {
    value
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse_u32(value: Option<&String>) -> Option<u32> {
    value?.parse().ok()
}

const EXTENSIONS: [&str; 5] = ["svg", "png", "webp", "jpg", "jpeg"];

fn find_named_file(directory: &Path, name: &str) -> Option<PathBuf> {
    // A name that already carries an extension -- `Icon=foo.png`, which
    // the spec discourages and desktop files still write -- is that file.
    let named = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
        .then(|| directory.join(name));
    named
        .into_iter()
        .chain(
            EXTENSIONS
                .into_iter()
                .map(|extension| directory.join(format!("{name}.{extension}"))),
        )
        .find(|path| path.is_file())
}
