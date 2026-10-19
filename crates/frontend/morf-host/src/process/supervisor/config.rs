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
    execute_config_with(runtime, path, source, policy, screens, &[])
}

/// As [`execute_config_on`], with `runtime_path` searched after the
/// configuration's own folder, as `MORF_RUNTIME_PATH` entries are.
pub fn execute_config_with(
    runtime: &mut Runtime,
    path: &Path,
    source: &[u8],
    policy: LoadPolicy,
    screens: &[Screen],
    runtime_path: &[PathBuf],
) -> Result<(), String> {
    let canonical = morf_lua::canonical_path(path);
    let mut roots = runtimepath_roots(&canonical, policy.external_roots);
    let after_config = roots.len().min(1);
    roots.splice(after_config..after_config, runtime_path.iter().cloned());
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
    runtime.set_config_file(&canonical);
    runtime.set_shell_root(
        canonical
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    );
    // Before any of it runs: a shell made for a newer engine is refused.
    runtime.check_shell_manifest(&canonical)?;
    for plugin in policy
        .plugins
        .then(|| runtime_scripts(&roots, "plugin"))
        .into_iter()
        .flatten()
    {
        match fs::read(&plugin) {
            Ok(source) => {
                if let Err(error) = runtime.execute_plugin(&plugin, &source) {
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
                if let Err(error) = runtime.execute_plugin(&after, &source) {
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
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let manifest = name == "permissions.lua" || name.ends_with(".permissions.lua");
        if path.is_dir() {
            collect_lua_scripts(&path, scripts);
        } else if name.ends_with(".lua") && !manifest {
            scripts.push(path);
        }
    }
}
