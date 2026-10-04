//! Lua modules: what is embedded, the runtime path, `require` and
//! `package.loaded`, and a project's library found above its configuration.

use std::fs;

use super::*;

#[test]
fn downstream_modules_are_not_embedded() {
    let mut runtime = Runtime::default();
    let error = runtime
        .execute("downstream.lua", b"require('consumer.widgets.button')")
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("module `consumer.widgets.button` is not available")
    );
}

#[test]
fn runtimepath_loads_user_modules_without_rust_registration() {
    let root = std::env::temp_dir().join(format!("morf-runtime-{}", std::process::id()));
    let module = root.join("lua/user/widget.lua");
    fs::create_dir_all(module.parent().unwrap()).unwrap();
    fs::write(&module, b"return { answer = 42 }").unwrap();
    let shell = root.join("shell.lua");
    let mut runtime = Runtime::default();

    runtime
        .execute(
            &shell.to_string_lossy(),
            b"local widget = require('user.widget'); assert(widget.answer == 42)",
        )
        .unwrap();

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn require_caches_user_modules_in_package_loaded() {
    let root = std::env::temp_dir().join(format!("morf-require-{}", std::process::id()));
    let module = root.join("lua/user/once.lua");
    fs::create_dir_all(module.parent().unwrap()).unwrap();
    fs::write(
        &module,
        b"module_runs = (module_runs or 0) + 1; return { runs = module_runs }",
    )
    .unwrap();
    let shell = root.join("shell.lua");
    let mut runtime = Runtime::default();

    runtime
        .execute(
            &shell.to_string_lossy(),
            br#"
                local first = require("user.once")
                local second = require("user.once")
                assert(first == second)
                assert(second.runs == 1)
                assert(package.loaded["user.once"] == first)
            "#,
        )
        .unwrap();

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_binding_can_require_a_module_that_holds_state() {
    // A module that keeps state creates its signals at its top level, and the
    // first `require` of it can come from anywhere — including a binding,
    // which runs inside a flush. The graph used to be taken out of reach for
    // the whole flush, so that first `require` failed with "reactive graph is
    // already running". Signals made mid-flush now join the flush.
    let root = std::env::temp_dir().join(format!("morf-lazy-{}", std::process::id()));
    let module = root.join("lua/user/lazy.lua");
    fs::create_dir_all(module.parent().unwrap()).unwrap();
    fs::write(
        &module,
        br#"
            local morf = require("morf")
            local M = {}
            M.count = morf.signal("lazy.count", 7)
            M.model = morf.state { label = "lazy" }
            M.count:set(8)
            return M
        "#,
    )
    .unwrap();
    let shell = root.join("shell.lua");
    let mut runtime = Runtime::default();
    runtime
        .execute(
            &shell.to_string_lossy(),
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local wanted = morf.signal("wanted", false)
                _G.label = ui.Text {
                    text = function()
                        if not wanted:get() then return "idle" end
                        local lazy = require("user.lazy")
                        return lazy.model.label .. ":" .. tostring(lazy.count:get())
                    end,
                }
                morf.ipc.want = function() wanted:set(true) end
                morf.ipc.bump = function() require("user.lazy").count:set(9) end
                morf.ipc.text = function() return _G.label.text end
            "#,
        )
        .unwrap();
    assert_eq!(
        runtime.call_ipc("text", &[]).unwrap(),
        [IpcValue::String("idle".to_owned())]
    );
    runtime.call_ipc("want", &[]).unwrap();
    let logs = runtime.take_logs();
    assert!(
        logs.iter()
            .all(|log| !log.message.contains("already running")),
        "{logs:?}"
    );
    assert_eq!(
        runtime.call_ipc("text", &[]).unwrap(),
        [IpcValue::String("lazy:8".to_owned())],
        "the module ran inside the binding and its signals were read"
    );
    // And the binding follows the signal the module made.
    runtime.call_ipc("bump", &[]).unwrap();
    assert_eq!(
        runtime.call_ipc("text", &[]).unwrap(),
        [IpcValue::String("lazy:9".to_owned())]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_configuration_finds_its_projects_library_above_it() {
    // A shell in a repository, folders deep, requires `lib.x` from the
    // repository's `library/` without a link beside it; one with no project
    // library above it finds none.
    let root = std::env::temp_dir().join(format!("morf-project-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let deep = root.join("examples/shells/demo/shell");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::create_dir_all(root.join("library/lib")).unwrap();
    std::fs::write(
        root.join("library/lib/greeting.lua"),
        "return 'from the project'",
    )
    .unwrap();
    let config = deep.join("init.lua");
    std::fs::write(&config, "").unwrap();
    let roots = crate::runtimepath_roots(&config, true);
    let library = std::fs::canonicalize(root.join("library")).unwrap();
    assert_eq!(roots[0], deep);
    assert_eq!(
        roots[1], library,
        "the project's library comes next: {roots:?}"
    );
    assert_eq!(
        crate::serialization::load_runtime_module(&roots, "lib.greeting").unwrap(),
        b"return 'from the project'"
    );
    // Asked to look nowhere else, it does not.
    assert_eq!(crate::runtimepath_roots(&config, false), vec![deep]);
    let _ = std::fs::remove_dir_all(&root);
}

mod compositor;
mod ipc_reload;
