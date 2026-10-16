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
fn imported_modules_record_their_origin_and_declaration() {
    let root = std::env::temp_dir().join(format!("morf-permissions-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    let imported_dir = root.join("site");
    fs::create_dir_all(&config_dir).unwrap();
    fs::create_dir_all(imported_dir.join("plug")).unwrap();
    fs::create_dir_all(config_dir.join("localmod")).unwrap();
    fs::write(
        imported_dir.join("plug/widget.lua"),
        b"return { answer = 42 }",
    )
    .unwrap();
    fs::write(
        imported_dir.join("plug/permissions.lua"),
        b"return { dbus = true, run = true }",
    )
    .unwrap();
    fs::write(config_dir.join("localmod/helper.lua"), b"return {}").unwrap();
    let config = config_dir.join("shell.lua");
    let mut runtime = Runtime::default();
    runtime.set_module_roots(vec![config_dir.clone(), imported_dir.clone()]);
    runtime
        .execute(
            &config.to_string_lossy(),
            b"local w = require('plug.widget'); assert(w.answer == 42); require('localmod.helper')",
        )
        .unwrap();
    let declarations = runtime.module_declarations();
    let widget = declarations
        .iter()
        .find(|record| record.name == "plug.widget")
        .unwrap();
    assert!(widget.imported);
    assert!(widget.declared);
    assert_eq!(widget.groups, ["dbus", "run"]);
    assert!(widget.unknown_groups().is_empty());
    let helper = declarations
        .iter()
        .find(|record| record.name == "localmod.helper")
        .unwrap();
    assert!(!helper.imported);
    assert!(!helper.declared);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn approvals_lapse_when_code_or_declaration_changes() {
    use morf_lua::{Approvals, ModuleDeclaration, module_fingerprint as fingerprint};

    let root = std::env::temp_dir().join(format!("morf-approvals-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let directory = root.join("plug");
    fs::create_dir_all(&directory).unwrap();
    let entry = directory.join("widget.lua");
    fs::write(&entry, b"return { answer = 42 }").unwrap();
    fs::write(directory.join("permissions.lua"), b"return { run = true }").unwrap();
    let record = ModuleDeclaration {
        name: "plug.widget".to_owned(),
        path: entry.clone(),
        imported: true,
        declared: true,
        groups: vec!["run".to_owned()],
    };
    let store_path = root.join("state/permissions-approvals.json");
    let mut approvals = Approvals::load(&store_path).unwrap();
    assert!(!approvals.is_approved(&record));
    let first = approvals.approve(&record).unwrap();
    assert_eq!(fingerprint(&record).unwrap(), first);
    assert!(approvals.is_approved(&record));
    approvals.save(&store_path).unwrap();
    let loaded = Approvals::load(&store_path).unwrap();
    assert!(
        loaded.is_approved(&record),
        "approval survives a round trip"
    );

    // Editing the code lapses it; editing the declaration lapses it too.
    fs::write(&entry, b"return { answer = 43 }").unwrap();
    assert!(!loaded.is_approved(&record));
    fs::write(&entry, b"return { answer = 42 }").unwrap();
    let changed = ModuleDeclaration {
        groups: vec!["dbus".to_owned(), "run".to_owned()],
        ..record.clone()
    };
    assert!(!loaded.is_approved(&changed));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_unapproved_imported_module_is_gated_at_load_time() {
    use morf_lua::{Approvals, ModuleDeclaration};

    let root = std::env::temp_dir().join(format!("morf-gate-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    let imported_dir = root.join("site");
    fs::create_dir_all(&config_dir).unwrap();
    fs::create_dir_all(imported_dir.join("plug")).unwrap();
    let probe = b"local child, err = morf.spawn { command = { \"printf\", \"x\" } }\n\
                  return { child = child, err = err }";
    fs::write(imported_dir.join("plug/widget.lua"), probe).unwrap();
    fs::write(
        imported_dir.join("plug/permissions.lua"),
        b"return { run = true }",
    )
    .unwrap();
    fs::write(config_dir.join("helper.lua"), probe).unwrap();
    let config = config_dir.join("shell.lua");

    // Unapproved: the process call at load is refused, the shell lives.
    let mut runtime = Runtime::default();
    runtime.set_module_roots(vec![config_dir.clone(), imported_dir.clone()]);
    runtime
        .execute(
            &config.to_string_lossy(),
            b"local w = require('plug.widget')\n\
              assert(w.child == nil, 'unapproved load-time run must be refused')\n\
              assert(tostring(w.err):find('permission', 1, true), tostring(w.err))\n\
              local h = require('helper')\n\
              assert(h.child ~= nil, 'config-local code is not gated: ' .. tostring(h.err))",
        )
        .unwrap();

    // Approved: the same module loads ungated in a fresh runtime.
    let record = ModuleDeclaration {
        name: "plug.widget".to_owned(),
        path: imported_dir.join("plug/widget.lua"),
        imported: true,
        declared: true,
        groups: vec!["run".to_owned()],
    };
    let store_path = runtime.approvals_path().unwrap();
    let mut approvals = Approvals::load(&store_path).unwrap();
    approvals.approve(&record).unwrap();
    approvals.save(&store_path).unwrap();
    let mut runtime = Runtime::default();
    runtime.set_module_roots(vec![config_dir.clone(), imported_dir.clone()]);
    runtime
        .execute(
            &config.to_string_lossy(),
            b"local w = require('plug.widget')\n\
              assert(w.child ~= nil, 'approved module runs at load')",
        )
        .unwrap();
    fs::remove_dir_all(root).unwrap();
}

mod compositor;
mod ipc_reload;
