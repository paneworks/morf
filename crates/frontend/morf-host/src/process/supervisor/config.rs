//! Running a configuration: its main file, and the scripts found on the
//! runtime path.

use morf_lua::{Runtime, Screen};
use std::fs;
use std::path::{Path, PathBuf};

use morf_lua::runtimepath_roots;

use super::{LoadPolicy, known_outputs};

pub fn execute_config(
    runtime: &mut Runtime,
    path: &Path,
    source: &[u8],
    policy: LoadPolicy,
) -> Result<(), String> {
    execute_config_on(runtime, path, source, policy, &known_outputs())
}

/// As [`execute_config`], with `morf.screens` given `screens` rather than
/// the recorded outputs: the outputless runtime has none by definition.
pub fn execute_config_on(
    runtime: &mut Runtime,
    path: &Path,
    source: &[u8],
    policy: LoadPolicy,
    screens: &[Screen],
) -> Result<(), String> {
    let roots = runtimepath_roots(path, policy.external_roots);
    // Applied before any Lua runs, so a configuration can measure itself
    // against the whole monitor layout while it loads. Index 1 of
    // `morf.screens` stays this runtime's own output.
    if runtime
        .capabilities()
        .iter()
        .any(|value| value == "desktop_canvas=true")
    {
        runtime.replace_screens(screens);
    } else {
        runtime.set_screens(screens);
    }
    runtime.set_module_roots(roots.clone());
    runtime.set_shell_root(
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    );
    for plugin in policy
        .plugins
        .then(|| runtime_scripts(&roots, "plugin"))
        .into_iter()
        .flatten()
    {
        match fs::read(&plugin) {
            Ok(source) => {
                if let Err(error) = runtime.execute(&plugin.to_string_lossy(), &source) {
                    eprintln!("morf: plugin {}: {error}", plugin.display());
                }
            }
            Err(error) => eprintln!("morf: plugin {}: {error}", plugin.display()),
        }
    }
    runtime
        .execute(&path.to_string_lossy(), source)
        .map_err(|error| error.to_string())?;
    for after in policy
        .plugins
        .then(|| runtime_scripts(&roots, "after/plugin"))
        .into_iter()
        .flatten()
    {
        match fs::read(&after) {
            Ok(source) => {
                if let Err(error) = runtime.execute(&after.to_string_lossy(), &source) {
                    eprintln!("morf: after plugin {}: {error}", after.display());
                }
            }
            Err(error) => eprintln!("morf: after plugin {}: {error}", after.display()),
        }
    }
    Ok(())
}

pub fn runtime_scripts(roots: &[PathBuf], directory: &str) -> Vec<PathBuf> {
    let mut scripts = Vec::new();
    for root in roots {
        let mut found = Vec::new();
        collect_lua_scripts(&root.join(directory), &mut found);
        found.sort();
        for path in found {
            if !scripts.contains(&path) {
                scripts.push(path);
            }
        }
    }
    scripts
}

pub fn collect_lua_scripts(path: &Path, scripts: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_lua_scripts(&path, scripts);
        } else if path.extension().and_then(|value| value.to_str()) == Some("lua") {
            scripts.push(path);
        }
    }
}
