//! IPC handlers, list handlers, and state carried across a reload.

use super::*;

#[test]
fn ipc_registry_calls_named_bounded_handlers() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "ipc.lua",
            br#"
                morf.ipc["launcher.toggle"] = function(name, count)
                    return "hello " .. name, count + 1, true
                end
            "#,
        )
        .unwrap();

    assert_eq!(runtime.ipc_verbs(), ["launcher.toggle"]);
    assert_eq!(
        runtime
            .call_ipc(
                "launcher.toggle",
                &[IpcValue::String("morf".into()), IpcValue::Integer(2)],
            )
            .unwrap(),
        [
            IpcValue::String("hello morf".into()),
            IpcValue::Integer(3),
            IpcValue::Boolean(true),
        ]
    );
    assert!(runtime.call_ipc("missing", &[]).is_err());
}

#[test]
fn ipc_handlers_are_fuel_bounded() {
    let mut runtime = Runtime::new(Limits {
        effect_fuel: 256,
        ..Limits::default()
    });
    runtime
        .execute(
            "ipc-fuel.lua",
            b"morf.ipc.loop = function() while true do end end",
        )
        .unwrap();

    let error = runtime.call_ipc("loop", &[]).unwrap_err().to_string();
    assert!(error.contains("IPC handler fuel exhausted"), "{error}");
}

#[test]
fn list_handlers_preserve_order_and_isolate_failures() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "handlers.lua",
            br#"
                local calls = ""
                morf.idle.subscribe(1000, function()
                  calls = calls .. "a"
                  error("broken")
                end)
                morf.idle.subscribe(1000, function()
                  calls = calls .. "b"
                end)
                morf.ipc["calls"] = function() return calls end
            "#,
        )
        .unwrap();

    assert!(runtime.dispatch_idle(1000, false, true));
    assert_eq!(
        runtime.call_ipc("calls", &[]).unwrap(),
        [IpcValue::String("ab".into())]
    );
    assert!(runtime.take_logs()[0].message.contains("broken"));
}

#[test]
fn reloadable_signals_carry_state_into_a_new_runtime() {
    let source = br#"
        local visible = morf.reloadable("launcher.visible", false)
        morf.ipc["state.set"] = function(value) visible:set(value) end
        morf.ipc["state.get"] = function() return visible:get() end
    "#;
    let mut first = Runtime::default();
    first.execute("reloadable.lua", source).unwrap();
    first
        .call_ipc("state.set", &[IpcValue::Boolean(true)])
        .unwrap();

    let mut second = Runtime::default();
    second.restore_reloadable_state(first.reloadable_state());
    second.execute("reloadable.lua", source).unwrap();

    assert_eq!(
        second.call_ipc("state.get", &[]).unwrap(),
        [IpcValue::Boolean(true)]
    );
}

#[test]
fn lua_reload_requests_are_coalesced_and_consumed() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "reload-request.lua",
            br#"
                local core = require("morf.core")
                core.reload(false)
                core.reload(true)
            "#,
        )
        .unwrap();

    assert_eq!(runtime.take_reload_request(), Some(true));
    assert_eq!(runtime.take_reload_request(), None);
}

#[test]
fn persistent_properties_reload_as_one_typed_scope() {
    let source = br#"
        local state = morf.persistent("launcher", { visible = false, page = 1 })
        morf.ipc["state.set"] = function()
            state.visible = true
            state.page = 4
        end
        morf.ipc["state.get"] = function()
            return state.visible, state.page, state.loaded, state.reloaded
        end
    "#;
    let mut first = Runtime::default();
    first.execute("persistent.lua", source).unwrap();
    assert_eq!(
        first.call_ipc("state.get", &[]).unwrap(),
        [
            IpcValue::Boolean(false),
            IpcValue::Integer(1),
            IpcValue::Boolean(true),
            IpcValue::Boolean(false),
        ]
    );
    first.call_ipc("state.set", &[]).unwrap();

    let mut second = Runtime::default();
    second.restore_reloadable_state(first.reloadable_state());
    second.execute("persistent.lua", source).unwrap();
    assert_eq!(
        second.call_ipc("state.get", &[]).unwrap(),
        [
            IpcValue::Boolean(true),
            IpcValue::Integer(4),
            IpcValue::Boolean(true),
            IpcValue::Boolean(true),
        ]
    );
}

#[test]
fn reload_scopes_isolate_repeated_local_ids() {
    let source = br#"
        local left = morf.scope("screen.left")
        local right = morf.scope("screen.right")
        local left_open = left:reloadable("open", false)
        local right_open = right:reloadable("open", true)
        local state = left:persistent("panel", { page = 2 })
        morf.ipc["scope.get"] = function()
            return left_open:get(), right_open:get(), state.page,
                left:id("open"), right:id("open")
        end
    "#;
    let mut runtime = Runtime::default();
    runtime.execute("scopes.lua", source).unwrap();
    assert_eq!(
        runtime.call_ipc("scope.get", &[]).unwrap(),
        [
            IpcValue::Boolean(false),
            IpcValue::Boolean(true),
            IpcValue::Integer(2),
            IpcValue::String("screen.left.open".into()),
            IpcValue::String("screen.right.open".into()),
        ]
    );
    assert_eq!(
        runtime
            .reloadable_state()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        [
            "screen.left.open".to_owned(),
            "screen.left.panel.page".to_owned(),
            "screen.right.open".to_owned(),
        ]
    );
}
