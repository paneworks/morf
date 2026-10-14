//! `morf.kit.native` controls live as long as the node they belong to.
#![cfg(feature = "kit")]

use std::path::Path;

use super::*;

fn count(runtime: &mut Runtime) -> IpcValue {
    runtime.call_ipc("count", &[]).unwrap().remove(0)
}

#[test]
fn a_control_goes_with_its_node_and_can_still_be_dropped_first() {
    register_kit();
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "kit.lua",
            br#"
                local ui = require("morf.ui")
                local native = require("morf.kit.native")
                morf.ipc.count = function() return native.count() end
                local owner = ui.Item { ui.Item {} }
                local inner = ui.Item {}
                ui.reparent(inner, owner)
                native.own((native.new("Press", {})), owner)
                native.own((native.new("Press", {})), inner)
                local early = native.new("Press", {})
                native.own(early, owner)
                native.drop(early)
                local kept = native.new("Press", {})
                assert(native.count() == 3, "three made, one dropped")
                morf.ipc.remove = function() ui.destroy(owner) end
                morf.ipc.drop = function() native.drop(kept) end
            "#,
        )
        .unwrap();
    runtime.call_ipc("remove", &[]).unwrap();
    assert_eq!(count(&mut runtime), IpcValue::Integer(1), "both owned went");
    runtime.call_ipc("drop", &[]).unwrap();
    assert_eq!(count(&mut runtime), IpcValue::Integer(0), "dropped by hand");
}

#[test]
fn a_headless_control_goes_with_its_owner() {
    register_kit();
    let mut runtime = Runtime::default();
    runtime.set_module_roots(vec![
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../library"),
    ]);
    runtime
        .execute(
            "headless.lua",
            br#"
                local ui = require("morf.ui")
                local native = require("morf.kit.native")
                local control = require("lib.kit.control")
                morf.ipc.count = function() return native.count() end
                local owner = ui.Item {}
                control.headless("Press", { owner = owner })
                assert(native.count() == 1)
                morf.ipc.remove = function() ui.destroy(owner) end
            "#,
        )
        .unwrap();
    runtime.call_ipc("remove", &[]).unwrap();
    assert_eq!(count(&mut runtime), IpcValue::Integer(0));
}
