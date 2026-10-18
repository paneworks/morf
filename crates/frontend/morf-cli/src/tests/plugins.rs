// Plugins: run before the shell and after it, past each other's failures,
// and not at all when the policy says so.

use morf_host::process::supervisor::execute_config;
use morf_lua::Runtime;
use std::fs;

use crate::*;

#[test]
fn config_executes_plugins_before_shell_and_after_last() {
    let root = std::env::temp_dir().join(format!("morf-plugins-{}", std::process::id()));
    fs::create_dir_all(root.join("plugin")).unwrap();
    fs::create_dir_all(root.join("after/plugin/nested")).unwrap();
    fs::write(root.join("plugin/first.lua"), b"plugin_value = 40").unwrap();
    fs::write(
        root.join("after/plugin/nested/last.lua"),
        b"assert(shell_value == 42); after_value = 43",
    )
    .unwrap();
    let shell = root.join("shell.lua");
    let source = b"assert(plugin_value == 40); shell_value = 42; morf.ui.Item {}";
    let mut runtime = Runtime::default();

    execute_config(&mut runtime, &shell, source, LoadPolicy::default()).unwrap();

    assert_eq!(runtime.scene().roots().len(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn plugin_failures_do_not_stop_later_plugins() {
    let root = std::env::temp_dir().join(format!("morf-plugin-errors-{}", std::process::id()));
    fs::create_dir_all(root.join("plugin")).unwrap();
    fs::write(root.join("plugin/01-broken.lua"), b"error('broken')").unwrap();
    fs::write(root.join("plugin/02-working.lua"), b"plugin_value = 42").unwrap();
    let shell = root.join("shell.lua");
    let mut runtime = Runtime::default();

    execute_config(
        &mut runtime,
        &shell,
        b"assert(plugin_value == 42); morf.ui.Item {}",
        LoadPolicy::default(),
    )
    .unwrap();

    assert_eq!(runtime.scene().roots().len(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn no_plugin_policy_skips_discovered_plugins() {
    let root = std::env::temp_dir().join(format!("morf-no-plugin-{}", std::process::id()));
    fs::create_dir_all(root.join("plugin")).unwrap();
    fs::write(root.join("plugin/entry.lua"), b"plugin_loaded = true").unwrap();
    let shell = root.join("shell.lua");
    let mut runtime = Runtime::default();

    execute_config(
        &mut runtime,
        &shell,
        b"assert(plugin_loaded == nil); morf.ui.Item {}",
        LoadPolicy {
            plugins: false,
            external_roots: true,
        },
    )
    .unwrap();

    assert_eq!(runtime.scene().roots().len(), 1);
    fs::remove_dir_all(root).unwrap();
}
