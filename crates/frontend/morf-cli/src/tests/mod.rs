use crate::supervisor::execute_config;
use crate::supervisor::lua_snapshot;
use crate::supervisor::named_screens;
use crate::supervisor::runtimepath_roots;
use crate::surface_popups::window_surface_effectively_visible;
use crate::surfaces::primary_surface_root;
use morf_io::IpcRequest;
use morf_io::IpcValue as WireValue;
use morf_lua::Runtime;
use morf_wayland::ScreenInfo;
use morf_wayland::physical_size;
use std::fs;
use std::path::PathBuf;

use crate::*;

mod headless;
mod layout_cache;
mod lock_input;
mod lock_ipc;
mod lock_trees;
mod operations;
mod supervision;
mod wake_plan;
mod wheel;

use std::collections::{HashMap, HashSet};

#[test]
fn named_screen_set_tracks_hotplug_identity() {
    let screens = [
        ScreenInfo {
            id: 7,
            name: Some("eDP-1".to_owned()),
            position: Some((0, 0)),
            size: Some((1920, 1080)),
            scale: 1,
            ..ScreenInfo::default()
        },
        ScreenInfo {
            id: 9,
            name: Some("DP-2".to_owned()),
            position: Some((1920, 0)),
            size: Some((2560, 1440)),
            scale: 2,
            ..ScreenInfo::default()
        },
    ];

    let names = named_screens(&screens).unwrap();

    assert_eq!(names.keys().cloned().collect::<Vec<_>>(), ["DP-2", "eDP-1"]);
    assert_eq!(names["DP-2"].id, 9);
}

#[test]
fn primary_root_excludes_registered_window_roots() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "window-roots.lua",
            br#"
                    local ui = require("morf.ui")
                    local window = require("morf.window")
                    local primary = ui.Item {}
                    local popup = ui.Item {}
                    window.popup { root = popup, width = 20, height = 10 }
                "#,
        )
        .unwrap();
    let primary = primary_surface_root(&runtime).unwrap();
    assert_eq!(runtime.scene().roots()[0], primary);
    assert_eq!(physical_size((101, 31), 150), (127, 39));
}

#[test]
fn child_window_visibility_follows_parent_chain() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "window-parents.lua",
            br#"
                    local ui = require("morf.ui")
                    local window = require("morf.window")
                    local parent = window.floating {
                      root = ui.Item {}, visible = false,
                    }
                    local child = window.floating {
                      root = ui.Item {}, visible = true, parent = parent,
                    }
                    window.popup {
                      root = ui.Item {}, visible = true, parent = child,
                    }
                "#,
        )
        .unwrap();
    let surfaces = runtime.window_surface_configs();
    let by_id = surfaces
        .iter()
        .map(|surface| (surface.id, surface))
        .collect::<HashMap<_, _>>();

    assert!(!window_surface_effectively_visible(
        2,
        &by_id,
        &mut HashSet::new()
    ));
    runtime.set_window_surface_visible(0, true);
    let surfaces = runtime.window_surface_configs();
    let by_id = surfaces
        .iter()
        .map(|surface| (surface.id, surface))
        .collect::<HashMap<_, _>>();
    assert!(window_surface_effectively_visible(
        2,
        &by_id,
        &mut HashSet::new()
    ));
}

#[test]
fn command_parser_exposes_ipc_and_legacy_config_path() {
    let args = ["ipc", "call", "launcher.toggle", "one", "two"].map(std::ffi::OsString::from);
    let Command::Client(IpcRequest::Call { target, args }) = parse_command(&args).unwrap() else {
        panic!("expected IPC call");
    };
    assert_eq!(target, "launcher.toggle");
    assert_eq!(
        args,
        [
            WireValue::String("one".into()),
            WireValue::String("two".into())
        ]
    );

    let args = [std::ffi::OsString::from("custom.lua")];
    let Command::Run(path, policy, _, _) = parse_command(&args).unwrap() else {
        panic!("expected config path");
    };
    assert_eq!(path, PathBuf::from("custom.lua"));
    assert_eq!(policy, LoadPolicy::default());

    let args = ["--no-plugin", "custom.lua"].map(std::ffi::OsString::from);
    let Command::Run(_, policy, _, _) = parse_command(&args).unwrap() else {
        panic!("expected config path");
    };
    assert!(!policy.plugins);
    assert!(policy.external_roots);

    let args = ["--clean", "custom.lua"].map(std::ffi::OsString::from);
    let Command::Run(_, policy, _, _) = parse_command(&args).unwrap() else {
        panic!("expected config path");
    };
    assert!(!policy.plugins);
    assert!(!policy.external_roots);

    // A file named lock remains accessible explicitly; the bare word now
    // selects the lock part of the default configuration.
    let args = ["./lock"].map(std::ffi::OsString::from);
    let Command::Run(path, _, _, _) = parse_command(&args).unwrap() else {
        panic!("expected config path");
    };
    assert_eq!(path, PathBuf::from("./lock"));

    // Everything after `--` is the configuration's, and nothing before it is.
    let args = ["shell.lua", "--", "-d", "lock"].map(std::ffi::OsString::from);
    let Command::Run(_, _, arguments, daemonize) = parse_command(&args).unwrap() else {
        panic!("expected config path");
    };
    assert_eq!(arguments, vec!["-d", "lock"]);
    assert!(!daemonize);
    let args = ["shell.lua", "--lock"].map(std::ffi::OsString::from);
    let error = parse_command(&args).unwrap_err();
    assert!(error.contains("after `--`"), "{error}");
    // A plain word is no exception: the separator is the rule, not the dash.
    let args = ["greeter.lua", "lock"].map(std::ffi::OsString::from);
    let error = parse_command(&args).unwrap_err();
    assert!(error.contains("`lock`"), "{error}");

    let args = ["log", "--bindings"].map(std::ffi::OsString::from);
    assert!(matches!(
        parse_command(&args).unwrap(),
        Command::Client(IpcRequest::Bindings)
    ));
}

#[test]
fn runtimepath_snapshot_tracks_nested_lua_changes() {
    let root = std::env::temp_dir().join(format!("morf-watch-{}", std::process::id()));
    let module = root.join("lua/plugin/widget.lua");
    fs::create_dir_all(module.parent().unwrap()).unwrap();
    fs::write(&module, b"return 1").unwrap();
    let before = lua_snapshot(std::slice::from_ref(&root));

    fs::write(&module, b"return 200").unwrap();
    let after = lua_snapshot(std::slice::from_ref(&root));

    assert_ne!(before, after);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lua_file_changes_are_pushed_and_other_files_ignored() {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    let root = std::env::temp_dir().join(format!("morf-follow-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("lua")).unwrap();
    fs::write(root.join("init.lua"), b"return 1").unwrap();
    let enabled = Arc::new(AtomicBool::new(true));
    let (tx, rx) = mpsc::channel();
    let thread_root = root.clone();
    let thread_enabled = Arc::clone(&enabled);
    std::thread::spawn(move || {
        crate::supervisor::follow_lua_files(&[thread_root], &thread_enabled, |_| {
            tx.send(()).is_ok()
        });
    });
    std::thread::sleep(Duration::from_millis(200));
    // Not a module: looked at, not a reload.
    fs::write(root.join("settings.json"), b"{}").unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
    // A module in a directory made after the watch began.
    fs::create_dir_all(root.join("lua/new")).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    fs::write(root.join("lua/new/widget.lua"), b"return 2").unwrap();
    assert!(rx.recv_timeout(Duration::from_secs(3)).is_ok());
    // A burst of saves is one reload.
    for index in 0..20 {
        fs::write(root.join("init.lua"), format!("return {index}")).unwrap();
    }
    assert!(rx.recv_timeout(Duration::from_secs(3)).is_ok());
    assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
    // Switched off: a change is taken in without a reload, and is not one
    // later either.
    enabled.store(false, std::sync::atomic::Ordering::Release);
    fs::write(root.join("init.lua"), b"return 'off'").unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
    enabled.store(true, std::sync::atomic::Ordering::Release);
    fs::write(root.join("notes.txt"), b"x").unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
    fs::remove_dir_all(root).unwrap();
}

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

#[test]
fn clean_policy_keeps_only_the_config_root() {
    let config = PathBuf::from("/tmp/morf-clean/shell.lua");

    assert_eq!(
        runtimepath_roots(&config, false),
        [PathBuf::from("/tmp/morf-clean")]
    );
}

/// Everything after the `--` belongs to the configuration. morf takes what it
/// needs to find the file and stops looking at the separator.
#[test]
fn arguments_after_the_separator_are_the_configurations_own() {
    let args =
        ["--clean", "custom.lua", "--", "--numbers-only", "-n", "5"].map(std::ffi::OsString::from);
    let Command::Run(path, policy, arguments, _) = parse_command(&args).unwrap() else {
        panic!("a configuration to run");
    };
    assert_eq!(path, std::path::PathBuf::from("custom.lua"));
    assert!(!policy.plugins);
    assert_eq!(arguments, ["--numbers-only", "-n", "5"]);
}

/// A word between the configuration and the `--` is nobody's: not one of
/// morf's, and not handed to a file that was never told to expect it.
#[test]
fn a_word_before_the_separator_is_refused() {
    let args = ["custom.lua", "greet", "--", "now"].map(std::ffi::OsString::from);
    let error = parse_command(&args).unwrap_err();
    assert!(error.contains("`greet`"), "{error}");
}

/// The `--` is morf getting out of the way, so a configuration can be asked
/// for its own help rather than morf answering for it.
#[test]
fn a_separator_hands_the_rest_over_untouched() {
    let args = ["custom.lua", "--", "--help"].map(std::ffi::OsString::from);
    let Command::Run(_, _, arguments, _) = parse_command(&args).unwrap() else {
        panic!("a configuration to run");
    };
    assert_eq!(arguments, ["--help"]);
}

/// An unknown option is an unknown option, not a filename. Passing the rest of
/// the line to the configuration must not turn a typo into "could not read
/// `--colour`".
#[test]
fn an_unknown_leading_option_is_still_refused() {
    let args = ["--colour", "red"].map(std::ffi::OsString::from);
    assert!(parse_command(&args).is_err());
}

#[test]
fn a_linked_configuration_runs_from_where_it_really_is() {
    // `make apply` makes `~/.config/morf/default` a link to a named shell:
    // its modules are beside the file linked to, not beside the link.
    let root = std::env::temp_dir().join(format!("morf-linked-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("caelestia/shell")).unwrap();
    fs::write(root.join("caelestia/shell/init.lua"), "").unwrap();
    std::os::unix::fs::symlink("caelestia", root.join("default")).unwrap();
    let real = fs::canonicalize(root.join("caelestia/shell/init.lua")).unwrap();
    let linked = crate::config::followed(root.join("default/shell/init.lua"));
    assert_eq!(linked, real);
    assert_eq!(
        runtimepath_roots(&linked, false),
        vec![real.parent().unwrap().to_path_buf()]
    );
    // One that is not there stays as given, for the error to name it.
    assert_eq!(
        crate::config::followed(PathBuf::from("no-such.lua")),
        PathBuf::from("no-such.lua")
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_named_shell_has_parts() {
    let root = std::env::temp_dir().join(format!("morf-parts-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    // SAFETY: the tests that read XDG_CONFIG_HOME are this one alone.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &root) };
    let morf = root.join("morf");
    let named = |name: &str| crate::config::named_config_path(name);
    assert_eq!(
        named("caelestia").unwrap(),
        morf.join("caelestia/shell/init.lua")
    );
    assert_eq!(
        named("caelestia/lock").unwrap(),
        morf.join("caelestia/lock/init.lua")
    );
    assert_eq!(
        named("caelestia/greet").unwrap(),
        morf.join("caelestia/greet/init.lua")
    );
    assert!(named("caelestia/bar").is_err());
    assert!(named("a/b/c").is_err());
    assert!(named("..").is_err());
    for part in ["shell", "lock", "greet"] {
        let args =
            [part, "-c", "caelestia", "--", "window", "preview"].map(std::ffi::OsString::from);
        let Command::Run(path, _, operands, _) = parse_command(&args).unwrap() else {
            panic!("expected role run")
        };
        assert_eq!(path, morf.join(format!("caelestia/{part}/init.lua")));
        assert_eq!(operands, ["window", "preview"]);
    }
    assert!(
        parse_command(&["lock", "-c", "caelestia/greet"].map(std::ffi::OsString::from)).is_err()
    );
    assert!(parse_command(&["greet", "unexpected"].map(std::ffi::OsString::from)).is_err());
    let Command::Run(path, _, _, _) = parse_command(&["lock".into()]).unwrap() else {
        panic!("expected lock")
    };
    assert_eq!(path, morf.join("default/lock/init.lua"));
    // The old layout, NAME/shell.lua, is still found.
    fs::create_dir_all(morf.join("old")).unwrap();
    fs::write(morf.join("old/shell.lua"), "").unwrap();
    assert_eq!(named("old").unwrap(), morf.join("old/shell.lua"));
    // Bare: the default's shell, unless the old shell.lua is there.
    assert_eq!(
        crate::config::default_config_path().unwrap(),
        morf.join("default/shell/init.lua")
    );
    fs::write(morf.join("shell.lua"), "").unwrap();
    assert_eq!(
        crate::config::default_config_path().unwrap(),
        morf.join("shell.lua")
    );
    // A greeter with an empty home uses system configuration; user parts
    // still override it. Relative XDG search entries are never loaded.
    let previous_dirs = std::env::var_os("XDG_CONFIG_DIRS");
    let system = root.join("system");
    fs::create_dir_all(system.join("morf/caelestia/greet")).unwrap();
    fs::write(system.join("morf/caelestia/greet/init.lua"), "").unwrap();
    unsafe { std::env::set_var("XDG_CONFIG_DIRS", format!("relative:{}", system.display())) };
    assert_eq!(
        named("caelestia/greet").unwrap(),
        system.join("morf/caelestia/greet/init.lua")
    );
    fs::create_dir_all(morf.join("caelestia/greet")).unwrap();
    fs::write(morf.join("caelestia/greet/init.lua"), "").unwrap();
    assert_eq!(
        named("caelestia/greet").unwrap(),
        morf.join("caelestia/greet/init.lua")
    );
    unsafe {
        match previous_dirs {
            Some(value) => std::env::set_var("XDG_CONFIG_DIRS", value),
            None => std::env::remove_var("XDG_CONFIG_DIRS"),
        }
    }
    unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
    let _ = fs::remove_dir_all(&root);
}
