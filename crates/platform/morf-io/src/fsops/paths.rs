//! Paths by text alone: normalising, `~` and `$VAR` expansion, and the user's
//! own directories.

use std::fs;
use std::path::{Component, Path, PathBuf};

/// `.` and `..` resolved by the text alone, without asking the disk.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if !out.pop() && !path.is_absolute() {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// `~` and `~/…` become the home directory; `$VAR` and `${VAR}` their value.
pub fn expand(path: &str) -> String {
    let mut text = path.to_owned();
    if let Some(home) = home_dir() {
        let home = home.to_string_lossy();
        if text == "~" {
            text = home.to_string();
        } else if let Some(rest) = text.strip_prefix("~/") {
            text = format!("{home}/{rest}");
        }
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let braced = chars.peek() == Some(&'{');
        if braced {
            chars.next();
        }
        let mut name = String::new();
        while let Some(&next) = chars.peek() {
            if next.is_ascii_alphanumeric() || next == '_' {
                name.push(next);
                chars.next();
            } else {
                break;
            }
        }
        if braced {
            if chars.peek() == Some(&'}') {
                chars.next();
            } else {
                out.push_str("${");
                out.push_str(&name);
                continue;
            }
        }
        if name.is_empty() {
            out.push('$');
            if braced {
                out.push_str("{}");
            }
        } else {
            out.push_str(&std::env::var(&name).unwrap_or_default());
        }
    }
    out
}

/// The user's directories: `config`, `data`, `cache`, `state`, `runtime`,
/// `home`, and the ones `user-dirs.dirs` names — `desktop`, `documents`,
/// `download`, `music`, `pictures`, `publicshare`, `templates`, `videos`.
pub fn user_dir(kind: &str) -> Option<PathBuf> {
    let home = home_dir();
    let env_or = |var: &str, fallback: &str| {
        std::env::var_os(var)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|home| home.join(fallback)))
    };
    match kind {
        "home" => home.clone(),
        "config" => env_or("XDG_CONFIG_HOME", ".config"),
        "data" => env_or("XDG_DATA_HOME", ".local/share"),
        "cache" => env_or("XDG_CACHE_HOME", ".cache"),
        "state" => env_or("XDG_STATE_HOME", ".local/state"),
        "runtime" => std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
        other => {
            let key = format!("XDG_{}_DIR", other.to_ascii_uppercase());
            // The environment first, as xdg-user-dir itself reads it: a
            // session that exports XDG_PICTURES_DIR means it.
            if let Some(value) = std::env::var_os(&key).filter(|value| !value.is_empty()) {
                return Some(PathBuf::from(expand(&value.to_string_lossy())));
            }
            let config = env_or("XDG_CONFIG_HOME", ".config")?;
            let text = fs::read_to_string(config.join("user-dirs.dirs")).ok();
            let named = text.as_deref().and_then(|text| {
                text.lines().find_map(|line| {
                    let (name, value) = line.trim().split_once('=')?;
                    (name.trim() == key).then(|| expand(value.trim().trim_matches('"')))
                })
            });
            if let Some(named) = named {
                return Some(PathBuf::from(named));
            }
            let fallback = match other {
                "desktop" => "Desktop",
                "documents" => "Documents",
                "download" | "downloads" => "Downloads",
                "music" => "Music",
                "pictures" => "Pictures",
                "publicshare" => "Public",
                "templates" => "Templates",
                "videos" => "Videos",
                _ => return None,
            };
            home.map(|home| home.join(fallback))
        }
    }
}
