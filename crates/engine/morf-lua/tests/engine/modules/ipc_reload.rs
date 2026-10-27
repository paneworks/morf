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
                morf.reload(false)
                morf.reload(true)
            "#,
        )
        .unwrap();

    let hard = ReloadRequest {
        hard: true,
        output: false,
    };
    assert_eq!(runtime.take_reload_request(), Some(hard));
    assert_eq!(runtime.take_reload_request(), None);
}

#[test]
fn a_reload_of_this_output_alone_gives_way_to_one_of_every_output() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "reload-output.lua",
            br#"
                morf.reload { output = true }
                morf.ipc.again = function() morf.reload { output = true } morf.reload() end
            "#,
        )
        .unwrap();

    let here = ReloadRequest {
        hard: false,
        output: true,
    };
    assert_eq!(runtime.take_reload_request(), Some(here));
    runtime.call_ipc("again", &[]).unwrap();
    assert_eq!(
        runtime.take_reload_request(),
        Some(ReloadRequest::default())
    );
    assert!(runtime.execute("bad.lua", b"morf.reload(3)").is_err());
}
