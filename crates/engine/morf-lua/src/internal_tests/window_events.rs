//! Tests of window_events.rs that reach the runtime's internals; the rest are in
//! tests/engine/window_events.rs.
#![allow(unused_imports)]

use super::*;

#[test]
fn a_destroyed_window_takes_its_tree_and_hears_closed_once() {
    let mut runtime = floating();
    let id = runtime.window_surface_configs()[0].id;
    let root = runtime.window_surface_configs()[0].root;
    let before = runtime.scene().node_count();
    runtime.take_window_surface_change();
    runtime
        .execute(
            "destroy.lua",
            br#"
                win:destroy()
                -- Once: a second destroy is nothing, and the shell closing the
                -- surface afterwards finds no callback left to run.
                win:destroy()
                morf.ipc.after = function()
                  local ok, err = pcall(win.visible, win)
                  return ok, tostring(err)
                end
                morf.ipc.open_again = function()
                  local ok, err = pcall(win.open, win)
                  return ok, tostring(err)
                end
            "#,
        )
        .unwrap();
    assert_eq!(ask(&mut runtime, "closed"), [IpcValue::Integer(1)]);
    assert!(
        !runtime.dispatch_window_closed(id),
        "nothing left to hear it"
    );
    assert_eq!(ask(&mut runtime, "closed"), [IpcValue::Integer(1)]);
    assert!(runtime.window_surface_configs().is_empty());
    assert!(runtime.take_window_surface_change());
    assert!(!runtime.scene().contains(root), "the root is gone");
    assert!(
        runtime.scene().node_count() < before - 1,
        "and what was under it"
    );
    for verb in ["after", "open_again"] {
        let answer = ask(&mut runtime, verb);
        assert_eq!(answer[0], IpcValue::Boolean(false));
        let IpcValue::String(message) = &answer[1] else {
            panic!("{answer:?}");
        };
        assert!(message.contains("window destroyed"), "{message}");
    }
    // Its size signals went with it.
    assert!(runtime.reactive.borrow().windows.window_sizes.is_empty());
}

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
