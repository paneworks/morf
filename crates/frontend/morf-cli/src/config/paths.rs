//! Where a configuration is: the config roots, named configurations and
//! their parts, the default, and the fonts carried beside one.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// The configuration where it really is: a link, or a folder that is one,
/// followed to the file. Its modules, fonts and watched folder are beside
/// that file -- so `~/.config/morf/default` can be a link to `caelestia` and
/// `require` finds caelestia's modules. A path that is not there is kept as
/// given, for the error to name.
pub(crate) fn followed(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

pub(crate) fn config_root() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(path).join("morf"));
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".config/morf"))
        .ok_or_else(|| "HOME and XDG_CONFIG_HOME are unset".to_owned())
}

/// User configuration first, then the system XDG configuration directories.
/// This lets the greeter account run the same installed shell without access
/// to another user's home or an embedded copy of the runtime.
fn config_roots() -> Result<Vec<PathBuf>, String> {
    let mut roots = vec![config_root()?];
    let dirs = env::var_os("XDG_CONFIG_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/etc/xdg".into());
    roots.extend(
        env::split_paths(&dirs)
            .filter(|path| path.is_absolute())
            .map(|path| path.join("morf")),
    );
    Ok(roots)
}

/// The parts one named shell may have, each a folder with its own
/// `init.lua`: the shell, its lock screen and its greeter.
pub(crate) const PARTS: [&str; 3] = ["shell", "lock", "greet"];

/// `-c NAME` or `-c NAME/PART`: `~/.config/morf/NAME/PART/init.lua`, the
/// part `shell` when none is named. A shell laid out the old way, as
/// `NAME/shell.lua`, is still found.
pub(crate) fn named_config_path(name: &str) -> Result<PathBuf, String> {
    let (name, part) = match name.split_once('/') {
        Some((name, part)) => (name, part),
        None => (name, "shell"),
    };
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return Err("config name must be NAME or NAME/PART".to_owned());
    }
    if !PARTS.contains(&part) {
        return Err(format!(
            "`{part}` is not a part of a shell: {}",
            PARTS.join(", ")
        ));
    }
    let roots = config_roots()?;
    for root in &roots {
        let folder = root.join(name);
        let path = folder.join(part).join("init.lua");
        if path.is_file() {
            return Ok(path);
        }
        let old = folder.join("shell.lua");
        if part == "shell" && old.is_file() {
            return Ok(old);
        }
    }
    Ok(roots[0].join(name).join(part).join("init.lua"))
}

/// What a bare `morf` runs: `~/.config/morf/shell.lua` if there is one (the
/// old layout), else the shell of whatever `~/.config/morf/default` is --
/// `make apply` makes that a link to a named shell.
pub(crate) fn default_config_path() -> Result<PathBuf, String> {
    let root = config_root()?;
    let old = root.join("shell.lua");
    if old.exists() {
        return Ok(old);
    }
    named_config_path("default/shell")
}

/// Puts a `fonts` directory beside the configuration, if there is one, on
/// the font path, ahead of whatever was there. A bundle carries its faces
/// this way, and a configuration on disk may keep its own beside it too.
/// Set before any renderer exists, which is when the font database is read,
/// and before any thread, which is when setting the environment is sound.
pub(super) fn carry_fonts(config: &Path) {
    let Some(fonts) = config.parent().map(|parent| parent.join("fonts")) else {
        return;
    };
    if !fonts.is_dir() {
        return;
    }
    let mut paths = vec![fonts];
    if let Some(existing) = env::var_os("MORF_FONT_PATH") {
        paths.extend(env::split_paths(&existing));
    }
    if let Ok(joined) = env::join_paths(paths) {
        // SAFETY: called from `run` before the supervisor starts its threads.
        unsafe { env::set_var("MORF_FONT_PATH", joined) };
    }
}
