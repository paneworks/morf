// A floating window hears the size the compositor configured it to, the
// request to close it, and that it went away.

use super::*;

fn floating() -> Runtime {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "window-events.lua",
            br#"
                local ui = require("morf.ui")
                local window = require("morf.window")
                local resized, requests, closed = {}, 0, 0
                local keep = false
                local root = ui.Item {}
                win = window.floating {
                  root = root, width = 640, height = 480, visible = true,
                  on_resize = function(w, h) resized[#resized + 1] = w .. "x" .. h end,
                }
                -- Content made once the handle exists follows the window
                -- through its bindings.
                content = ui.Rect {
                  width = function() return win.width end,
                  height = function() return win.height - 40 end,
                }
                ui.reparent(content, root)
                morf.ipc.content = function() return content.width, content.height end
                win:on_close_requested(function()
                  requests = requests + 1
                  if keep then return false end
                end)
                win:on_closed(function() closed = closed + 1 end)
                morf.ipc.resized = function() return table.concat(resized, ",") end
                morf.ipc.requests = function() return requests end
                morf.ipc.closed = function() return closed end
                morf.ipc.keep = function(value) keep = value end
                morf.ipc.visible = function() return win:visible() end
                morf.ipc.size = function() return win.width, win.height end
            "#,
        )
        .unwrap();
    runtime
}

fn ask(runtime: &mut Runtime, verb: &str) -> Vec<IpcValue> {
    runtime.call_ipc(verb, &[]).unwrap()
}

#[test]
fn a_floating_window_reads_its_configured_size() {
    let mut runtime = floating();
    let id = runtime.window_surface_configs()[0].id;
    // Until the compositor says otherwise, the size it asked for.
    assert_eq!(
        ask(&mut runtime, "size"),
        [IpcValue::Integer(640), IpcValue::Integer(480)]
    );
    assert_eq!(
        ask(&mut runtime, "content"),
        [IpcValue::Number(640.0), IpcValue::Number(440.0)]
    );

    assert!(runtime.set_window_surface_size(id, 900, 700));
    assert_eq!(
        ask(&mut runtime, "size"),
        [IpcValue::Integer(900), IpcValue::Integer(700)]
    );
    // The bindings that read it ran before anything is laid out.
    assert_eq!(
        ask(&mut runtime, "content"),
        [IpcValue::Number(900.0), IpcValue::Number(660.0)]
    );
    assert_eq!(
        ask(&mut runtime, "resized"),
        [IpcValue::String("900x700".into())]
    );

    // The same size again is no change and no callback.
    assert!(!runtime.set_window_surface_size(id, 900, 700));
    assert_eq!(
        ask(&mut runtime, "resized"),
        [IpcValue::String("900x700".into())]
    );
}

#[test]
fn a_close_request_hides_the_window_unless_it_is_refused() {
    let mut runtime = floating();
    let id = runtime.window_surface_configs()[0].id;
    runtime.take_window_surface_change();

    runtime
        .call_ipc("keep", &[IpcValue::Boolean(true)])
        .unwrap();
    assert!(!runtime.request_window_close(id));
    assert_eq!(ask(&mut runtime, "visible"), [IpcValue::Boolean(true)]);
    assert!(!runtime.take_window_surface_change());

    runtime
        .call_ipc("keep", &[IpcValue::Boolean(false)])
        .unwrap();
    assert!(runtime.request_window_close(id));
    assert_eq!(ask(&mut runtime, "visible"), [IpcValue::Boolean(false)]);
    assert!(runtime.take_window_surface_change());
    assert_eq!(ask(&mut runtime, "requests"), [IpcValue::Integer(2)]);

    // The host takes it down and says so.
    assert!(runtime.dispatch_window_closed(id));
    assert_eq!(ask(&mut runtime, "closed"), [IpcValue::Integer(1)]);
}

#[test]
fn a_window_with_no_close_handler_just_hides() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "window-plain.lua",
            br#"
                local ui = require("morf.ui")
                local window = require("morf.window")
                win = window.floating { root = ui.Item {}, visible = true }
                morf.ipc.visible = function() return win:visible() end
            "#,
        )
        .unwrap();
    let id = runtime.window_surface_configs()[0].id;
    assert!(runtime.request_window_close(id));
    assert_eq!(ask(&mut runtime, "visible"), [IpcValue::Boolean(false)]);
    assert!(!runtime.dispatch_window_closed(id));
}

#[test]
fn a_popup_hears_its_size_but_not_a_close_request() {
    let mut runtime = Runtime::default();
    let error = runtime
        .execute(
            "popup-close.lua",
            br#"
                local ui = require("morf.ui")
                local window = require("morf.window")
                local popup = window.popup { root = ui.Item {}, width = 200, height = 100 }
                popup:on_close_requested(function() end)
            "#,
        )
        .unwrap_err();
    assert!(error.to_string().contains("floating"), "{error}");

    let mut runtime = Runtime::default();
    runtime
        .execute(
            "popup-size.lua",
            br#"
                local ui = require("morf.ui")
                local window = require("morf.window")
                local sizes = {}
                popup = window.popup {
                  root = ui.Item {}, width = 200, height = 100,
                  on_resize = function(w, h) sizes[#sizes + 1] = w * 1000 + h end,
                }
                morf.ipc.sizes = function() return table.concat(sizes, ",") end
                morf.ipc.width = function() return popup.width end
            "#,
        )
        .unwrap();
    let id = runtime.window_surface_configs()[0].id;
    assert!(runtime.set_window_surface_size(id, 180, 90));
    assert_eq!(ask(&mut runtime, "width"), [IpcValue::Integer(180)]);
    assert_eq!(
        ask(&mut runtime, "sizes"),
        [IpcValue::String("180090".into())]
    );
}
