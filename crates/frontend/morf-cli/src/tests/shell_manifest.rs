// A shell's morf.lua: an engine too old for it refuses it before any of it
// runs; one past the release line it was tested on runs it and says so.

use morf_host::process::supervisor::execute_config;
use morf_lua::Runtime;
use std::fs;
use std::path::{Path, PathBuf};

use crate::*;

/// A shell with parts, as eva is laid out: `morf.lua` at the root, the
/// configuration in `shell/`.
fn shell(name: &str, manifest: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("morf-manifest-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("shell")).unwrap();
    fs::write(root.join("morf.lua"), manifest).unwrap();
    root.join("shell/init.lua")
}

fn load(config: &Path) -> (Runtime, Result<(), String>) {
    let mut runtime = Runtime::default();
    let result = execute_config(
        &mut runtime,
        config,
        b"ran = true; morf.ui.Item {}",
        LoadPolicy::default(),
    );
    (runtime, result)
}

fn remove(config: &Path) {
    let _ = fs::remove_dir_all(config.parent().unwrap().parent().unwrap());
}

#[test]
fn a_shell_for_a_newer_engine_is_refused_before_it_runs() {
    let config = shell(
        "newer",
        r#"return { name = "eva", version = "9.0.0", morf = ">=99" }"#,
    );
    let (runtime, result) = load(&config);
    let error = result.unwrap_err();
    assert!(
        error.starts_with("eva 9.0.0 needs morf >=99; this is morf "),
        "{error}"
    );
    assert!(runtime.scene().roots().is_empty(), "none of it ran");
    remove(&config);
}

#[test]
fn a_shell_the_engine_meets_runs() {
    let config = shell("met", r#"return { name = "eva", morf = ">=0.0.1" }"#);
    let (runtime, result) = load(&config);
    result.unwrap();
    assert_eq!(runtime.scene().roots().len(), 1);
    remove(&config);
}

#[test]
fn an_engine_past_the_tested_line_runs_the_shell_and_warns() {
    let config = shell("tested", r#"return { name = "eva", tested = "0.0" }"#);
    let (mut runtime, result) = load(&config);
    result.unwrap();
    let logs = runtime.take_logs();
    assert!(
        logs.iter().any(|entry| entry
            .message
            .starts_with("eva was tested up to morf 0.0; this is morf ")),
        "{logs:?}"
    );
    remove(&config);
}

#[test]
fn a_manifest_that_is_not_data_is_refused() {
    let config = shell("broken", "return 42");
    let (_, result) = load(&config);
    assert!(result.unwrap_err().contains("must return a table"));
    remove(&config);
    let config = shell("nonsense", r#"return { morf = ">=zero" }"#);
    let (_, result) = load(&config);
    assert!(result.unwrap_err().contains("`zero` is not a version"));
    remove(&config);
}

#[test]
fn lua_asks_the_same_question() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "satisfies.lua",
            b"assert(morf.satisfies('>=0.0.1')); assert(not morf.satisfies('>=99'))",
        )
        .unwrap();
}
