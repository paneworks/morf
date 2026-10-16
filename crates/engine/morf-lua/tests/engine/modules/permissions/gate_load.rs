//! What the gate holds: everything an unapproved module's top level causes
//! while it runs, and nothing it leaves behind once it has returned.

use morf_lua::{Approvals, ModuleKind, PermissionMode};

use super::{Fixture, appears, refusals, stays_absent, touch};

/// A module whose top level reaches `morf.spawn` every indirect way it can.
fn sneaky(mark: &str) -> String {
    let spawn = format!("morf.spawn {{ command = {{ 'touch', '{mark}' }} }}");
    format!(
        r#"
        local out = {{}}
        local function keep(name, child, why) out[name] = {{ child = child, why = why }} end
        keep("load", load("return {spawn}")())
        keep("coroutine", coroutine.wrap(function() return {spawn} end)())
        keep("pcall", select(2, pcall(morf.spawn, {{ command = {{ 'touch', '{mark}' }} }})))
        keep("xpcall", select(2, xpcall(function() return {spawn} end, function(e) return e end)))
        keep("metamethod", setmetatable({{}}, {{ __index = function() return ({spawn}) end }}).x)
        local native = setmetatable({{ command = {{ 'touch', '{mark}' }} }}, {{ __index = morf.spawn }})
        keep("native metamethod", native.anything)
        keep("helper", HELPER({{ 'touch', '{mark}' }}))
        keep("first require", table.unpack(require("firstload")))
        morf.ipc.late = function() return ({spawn}) ~= nil end
        return out
        "#
    )
}

#[test]
fn whatever_a_held_top_level_causes_is_held_too() {
    let fixture = Fixture::new("held-extent");
    let mark = fixture.out("spawned");
    let mark_text = mark.display().to_string();
    fixture.write("site/sneaky.lua", &sneaky(&mark_text));
    fixture.write(
        "config/firstload.lua",
        &format!("return {{ {} }}", touch(&mark)),
    );
    let mut runtime = fixture.runtime();
    fixture.run(
        &mut runtime,
        r#"
        function HELPER(command) return morf.spawn { command = command } end
        local out = require("sneaky")
        local leaked = {}
        for name, result in pairs(out) do
          local named = result.why == nil or tostring(result.why):find("`sneaky`", 1, true)
          if result.child ~= nil or not named then
            leaked[#leaked + 1] = name .. ": " .. tostring(result.why)
          end
        end
        table.sort(leaked)
        assert(#leaked == 0, table.concat(leaked, "\n"))
        "#,
    );
    assert!(stays_absent(&mark), "nothing ran while sneaky loaded");
    assert_eq!(refusals(&mut runtime).len(), 8, "one line for each way in");

    // Its top level has returned: what it left behind is not held.
    runtime.call_ipc("late", &[]).unwrap();
    assert!(appears(&mark), "after load nothing is gated");
}

#[test]
fn a_trusted_module_required_first_by_the_configuration_runs_free() {
    let fixture = Fixture::new("load-order");
    let mark = fixture.out("first");
    fixture.write(
        "config/firstload.lua",
        &format!("return {{ {} }}", touch(&mark)),
    );
    fixture.write("site/later.lua", "return require('firstload')");
    let mut runtime = fixture.runtime();
    fixture.run(
        &mut runtime,
        r#"
        local first = require("firstload")
        assert(first[1] ~= nil, tostring(first[2]))
        assert(require("later") == first, "the cached module, not run again")
        "#,
    );
    assert!(appears(&mark));
    assert!(refusals(&mut runtime).is_empty());
}

#[test]
fn plugin_scripts_outside_the_configuration_are_held() {
    let fixture = Fixture::new("plugins");
    let theirs = fixture.out("their-plugin");
    let mine = fixture.out("my-plugin");
    let evil = fixture.write("site/plugin/evil.lua", &touch(&theirs));
    let own = fixture.write("config/plugin/mine.lua", &touch(&mine));
    let mut runtime = fixture.runtime();
    runtime
        .execute_plugin(&evil, std::fs::read(&evil).unwrap().as_slice())
        .unwrap();
    runtime
        .execute_plugin(&own, std::fs::read(&own).unwrap().as_slice())
        .unwrap();
    assert!(appears(&mine), "the configuration's own plugin runs free");
    assert!(stays_absent(&theirs), "an imported plugin is held");
    let record = fixture.record(&runtime, "plugin/evil.lua");
    assert_eq!(record.kind, ModuleKind::Plugin);
    assert!(record.imported());
    assert_eq!(refusals(&mut runtime).len(), 1);
}

#[test]
fn check_holds_approved_modules_and_approve_refuses_everyone() {
    let fixture = Fixture::new("modes");
    let approved_mark = fixture.out("approved");
    let config_mark = fixture.out("config");
    fixture.write(
        "site/trusted.lua",
        &format!("return {{ {} }}", touch(&approved_mark)),
    );

    // Approved, and run free by the shell.
    let mut runtime = fixture.runtime();
    let record = runtime.declare_module("trusted").unwrap();
    let mut approvals = Approvals::load(&fixture.store()).unwrap();
    approvals.approve(&record).unwrap();
    approvals.save(&fixture.store()).unwrap();

    // `morf check` holds it all the same.
    let mut check = fixture.runtime();
    check.set_permission_mode(PermissionMode::HoldImported);
    fixture.run(&mut check, "assert(require('trusted')[1] == nil)");
    assert!(stays_absent(&approved_mark));

    // `morf approve` refuses the configuration itself too.
    let mut approve = fixture.runtime();
    approve.set_permission_mode(PermissionMode::RefuseAll);
    fixture.run(
        &mut approve,
        &format!(
            "local child, why = {}\nassert(child == nil and why:find('morf approve', 1, true))",
            touch(&config_mark)
        ),
    );
    assert!(stays_absent(&config_mark));

    fixture.run(&mut runtime, "assert(require('trusted')[1] ~= nil)");
    assert!(
        appears(&approved_mark),
        "the shell runs an approved module free"
    );
}
