use super::*;
use morf_lua::FocusReason;

/// A surface of buttons (`strong`), a scope of two more, a plain item, and a
/// hidden branch. Returns the runtime, the root and the nodes by name.
fn surface() -> (
    Runtime,
    NodeHandle,
    std::collections::HashMap<String, NodeHandle>,
) {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "focus.lua",
            br#"
                local ui = require("morf.ui")
                seen = {}
                local function button(name, extra)
                    local t = { id = name, focus_policy = "strong", width = 10, height = 10,
                        on_focus_changed = function(f) seen[#seen + 1] = name .. tostring(f) end }
                    for k, v in pairs(extra or {}) do t[k] = v end
                    return ui.Item(t)
                end
                ui.Item {
                    button("a"),
                    ui.Item { id = "scope", focus_scope = true, button("s1"), button("s2") },
                    ui.Item { id = "plain" },
                    ui.Item { id = "hidden", visible = false, button("h") },
                    button("click", { focus_policy = "click" }),
                    button("b"),
                }
                morf.ipc.seen = function() local s = table.concat(seen, ","); seen = {}; return s end
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let mut names = std::collections::HashMap::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if let Ok(id) = runtime.scene().string_value(node, "id")
            && !id.is_empty()
        {
            names.insert(id.to_owned(), node);
        }
        pending.extend(runtime.scene().children(node).unwrap().iter().copied());
    }
    (runtime, root, names)
}

fn flag(runtime: &Runtime, node: NodeHandle, property: &str) -> bool {
    runtime.scene().bool_value(node, property).unwrap()
}

#[test]
fn tab_walks_the_chain_in_tree_order_and_skips_what_cannot_hold_focus() {
    let (runtime, root, n) = surface();
    let mut order = Vec::new();
    let mut current = None;
    for _ in 0..4 {
        current = runtime.next_focus_in(root, current, false);
        order.push(current.unwrap());
    }
    // "click" takes focus by click only; "hidden"'s button is out of reach.
    assert_eq!(order, [n["a"], n["s1"], n["s2"], n["b"]]);
    assert_eq!(
        runtime.next_focus_in(root, Some(n["b"]), false),
        Some(n["a"])
    );
    assert_eq!(
        runtime.next_focus_in(root, Some(n["a"]), true),
        Some(n["b"])
    );
    assert_eq!(runtime.next_focus_in(root, None, true), Some(n["b"]));
}

#[test]
fn a_ring_shows_for_the_keyboard_and_never_for_a_click() {
    let (mut runtime, root, n) = surface();
    runtime.set_focus(root, Some(n["a"]), FocusReason::Keyboard);
    assert!(flag(&runtime, n["a"], "focused"));
    assert!(flag(&runtime, n["a"], "visual_focus"));
    runtime.set_focus(root, Some(n["b"]), FocusReason::Click);
    assert!(!flag(&runtime, n["a"], "focused"));
    assert!(!flag(&runtime, n["a"], "visual_focus"));
    assert!(flag(&runtime, n["b"], "focused"));
    assert!(!flag(&runtime, n["b"], "visual_focus"));
    assert_eq!(
        runtime.call_ipc("seen", &[]).unwrap(),
        [IpcValue::String("atrue,afalse,btrue".into())]
    );
    assert_eq!(runtime.focus_owner(root), Some(n["b"]));
}

#[test]
fn a_click_focuses_the_nearest_node_that_takes_it_by_click() {
    let (runtime, _, n) = surface();
    assert_eq!(runtime.click_focus_target(n["click"]), Some(n["click"]));
    assert_eq!(runtime.click_focus_target(n["plain"]), None);
    assert_eq!(runtime.click_focus_target(n["h"]), None);
}

#[test]
fn a_scope_remembers_and_is_left_whole() {
    let (mut runtime, root, n) = surface();
    runtime.set_focus(root, Some(n["s2"]), FocusReason::Keyboard);
    runtime.set_focus(root, Some(n["b"]), FocusReason::Keyboard);
    // Back into the scope: where it last had focus, not its first node.
    assert_eq!(
        runtime.next_focus_in(root, Some(n["b"]), true),
        Some(n["s2"])
    );
    assert_eq!(
        runtime.next_focus_in(root, Some(n["a"]), false),
        Some(n["s2"])
    );
    // Inside it Tab walks it; out of it, Tab goes to what is beside it.
    assert_eq!(
        runtime.next_focus_in(root, Some(n["s1"]), false),
        Some(n["s2"])
    );
    assert_eq!(
        runtime.next_focus_in(root, Some(n["s2"]), false),
        Some(n["b"])
    );
    assert_eq!(
        runtime.next_focus_in(root, Some(n["s1"]), true),
        Some(n["a"])
    );
}

#[test]
fn focus_lost_inside_a_scope_stays_inside_it() {
    let (mut runtime, root, n) = surface();
    runtime.set_focus(root, Some(n["s1"]), FocusReason::Keyboard);
    runtime.set_focus(root, Some(n["s2"]), FocusReason::Keyboard);
    runtime
        .scene_mut()
        .assign(n["s2"], "visible", morf_scene::Value::Bool(false))
        .unwrap();
    assert!(runtime.check_focus());
    assert_eq!(runtime.focus_owner(root), Some(n["s1"]));
    // A hand-off keeps the ring the keyboard had put there.
    assert!(flag(&runtime, n["s1"], "visual_focus"));
    // Out of any scope, focus goes nowhere.
    runtime.set_focus(root, Some(n["a"]), FocusReason::Click);
    runtime
        .scene_mut()
        .assign(n["a"], "enabled", morf_scene::Value::Bool(false))
        .unwrap();
    runtime.check_focus();
    assert_eq!(runtime.focus_owner(root), None);
}

#[test]
fn focus_shows_only_while_the_surface_has_the_keyboard() {
    let (mut runtime, root, n) = surface();
    runtime.set_focus(root, Some(n["a"]), FocusReason::Keyboard);
    runtime.set_focus_active(root, false);
    assert!(!flag(&runtime, n["a"], "focused"));
    assert_eq!(runtime.focus_owner(root), Some(n["a"]));
    runtime.set_focus_active(root, true);
    assert!(flag(&runtime, n["a"], "focused"));
}

#[test]
fn a_configuration_moves_focus_and_cannot_write_it() {
    let (mut runtime, root, n) = surface();
    let error = runtime
        .execute(
            "write.lua",
            br#"require("morf.ui").Item { focused = true }"#,
        )
        .unwrap_err();
    assert!(error.to_string().contains("read-only"), "{error}");
    runtime
        .execute(
            "move.lua",
            br#"
                morf.ipc.move = function() morf.focus.next() end
            "#,
        )
        .unwrap();
    runtime.set_focus(root, Some(n["a"]), FocusReason::Click);
    runtime.call_ipc("move", &[]).unwrap();
    runtime.check_focus();
    assert_eq!(runtime.focus_owner(root), Some(n["s1"]));
    assert!(flag(&runtime, n["s1"], "visual_focus"));
}
