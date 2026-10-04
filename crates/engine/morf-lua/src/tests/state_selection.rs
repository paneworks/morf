//! Which state is current: `when`, the default, `order`, and a `when`
//! state refused beside a state binding.

use crate::*;

#[test]
fn a_state_with_when_selects_itself_and_default_takes_over_otherwise() {
    // Hover and press used to be a signal each, written from four handlers
    // and read by a `state` binding somebody wrote by hand. A state that
    // says `when` chooses itself; `default` is where the node goes when
    // none does.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "when.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                local hover = morf.signal("hover", false)
                local down = morf.signal("down", false)
                ui.Rect {
                    width = 10, height = 10, color = "#000000",
                    states = {
                        default = { property_changes = { width = 10 } },
                        hovered = {
                            when = function() return hover:get() end,
                            property_changes = { width = 20 },
                        },
                        pressed = {
                            when = function() return down:get() end,
                            property_changes = { width = 5 },
                        },
                    },
                }
                morf.ipc.hover = function(on) hover:set(on) end
                morf.ipc.press = function(on) down:set(on) end
            "##,
        )
        .unwrap();
    let node = runtime.scene().roots()[0];
    assert_eq!(runtime.scene().number(node, "width").unwrap(), 10.0);

    runtime
        .call_ipc("hover", &[IpcValue::Boolean(true)])
        .unwrap();
    assert_eq!(runtime.scene().number(node, "width").unwrap(), 20.0);
    // Both true: name order, and `hovered` sorts before `pressed`.
    runtime
        .call_ipc("press", &[IpcValue::Boolean(true)])
        .unwrap();
    assert_eq!(runtime.scene().number(node, "width").unwrap(), 20.0);
    runtime
        .call_ipc("hover", &[IpcValue::Boolean(false)])
        .unwrap();
    assert_eq!(runtime.scene().number(node, "width").unwrap(), 5.0);
    runtime
        .call_ipc("press", &[IpcValue::Boolean(false)])
        .unwrap();
    assert_eq!(runtime.scene().number(node, "width").unwrap(), 10.0);
}

#[test]
fn a_when_state_and_a_state_binding_together_are_refused() {
    let mut runtime = Runtime::default();
    let error = runtime
        .execute(
            "when-conflict.lua",
            br#"
                local ui = require("morf.ui")
                ui.Rect {
                    states = { a = { when = function() return true end, property_changes = {} } },
                    state = "a",
                }
            "#,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("choose themselves"), "{error}");
}

#[test]
fn a_state_with_order_is_asked_before_its_alphabetical_betters() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "order.lua",
            br#"
                local ui = require("morf.ui")
                ui.Rect {
                    width = 10, height = 10,
                    states = {
                        alpha = { when = function() return true end, property_changes = { width = 1 } },
                        omega = { order = -1, when = function() return true end, property_changes = { width = 2 } },
                    },
                }
            "#,
        )
        .unwrap();
    let node = runtime.scene().roots()[0];
    assert_eq!(runtime.scene().number(node, "width").unwrap(), 2.0);
}
