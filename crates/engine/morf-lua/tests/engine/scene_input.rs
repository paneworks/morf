//! Input on scene nodes from Lua: a mouse area's click, key handlers with
//! repeats and releases, and keyboard focus through ancestors.

use super::*;

#[test]
fn mouse_area_emits_clicked() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "button.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local count = morf.signal("count", 0)
                ui.Item {
                    ui.Text { text = function() return "" .. count:get() end },
                    ui.MouseArea {
                        width = 80,
                        height = 24,
                        accepted_buttons = { "right", 274 },
                        on_clicked = function() count:set(count:get() + 1) end,
                    },
                }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let children = runtime.scene().children(root).unwrap().to_vec();

    assert!(!runtime.accepts_pointer_button(children[1], 0x110));
    assert!(runtime.accepts_pointer_button(children[1], 0x111));
    assert!(runtime.accepts_pointer_button(children[1], 0x112));
    assert!(runtime.dispatch_ui_event(children[1], UiEvent::Clicked));

    assert_eq!(
        runtime.scene().string_value(children[0], "text").unwrap(),
        "1"
    );
}

#[test]
fn key_handlers_receive_keysym_and_text() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "key.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local value = morf.signal("key", "")
                ui.Item {
                    ui.MouseArea {
                        width = 100,
                        height = 40,
                        on_key_pressed = function(keysym, text)
                            value:set(keysym .. ":" .. text)
                        end,
                    },
                    ui.Text { text = function() return value:get() end },
                }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let children = runtime.scene().children(root).unwrap().to_vec();

    assert!(runtime.dispatch_key_event(children[0], 65, Some("A")));
    assert_eq!(
        runtime.scene().string_value(children[1], "text").unwrap(),
        "65:A"
    );
}

#[test]
fn key_handlers_hear_repeats_and_releases() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "held.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local log = morf.signal("log", "")
                local function note(entry) log:set(log:get() .. entry .. ";") end
                ui.Item {
                    ui.MouseArea {
                        on_key_pressed = function(keysym, text, modifiers, repeat_, name)
                            note("down " .. keysym .. " " .. tostring(text) .. " "
                                .. modifiers .. " " .. tostring(repeat_) .. " " .. tostring(name))
                        end,
                        on_key_released = function(keysym, text, modifiers, extra, name)
                            note("up " .. keysym .. " " .. modifiers .. " " .. tostring(extra)
                                .. " " .. tostring(name))
                        end,
                    },
                    ui.MouseArea {
                        -- Releases alone make a node somewhere keys can go.
                        on_key_released = function() end,
                    },
                    ui.Text { text = function() return log:get() end },
                }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let children = runtime.scene().children(root).unwrap().to_vec();
    let shift = KeyModifiers {
        shift: true,
        ..KeyModifiers::default()
    };

    assert!(runtime.dispatch_key(children[0], 65, Some("A"), shift));
    assert!(runtime.dispatch_key_press(children[0], 65, Some("A"), shift, true));
    assert!(runtime.dispatch_key_release(children[0], 65, Some("A"), KeyModifiers::default()));
    // A key's name comes fifth, the X name `morf.keys` has it under.
    assert!(runtime.dispatch_key_press(children[0], 0xff54, None, KeyModifiers::default(), false));
    assert_eq!(
        runtime.scene().string_value(children[2], "text").unwrap(),
        "down 65 A shift false A;down 65 A shift true A;up 65  nil A;down 65364 nil  false Down;"
    );
    runtime
        .execute("keys.lua", br#"assert(morf.keys.Down == 65364 and morf.keys.Return == 65293 and morf.keys.F5 == 65474)"#)
        .unwrap();
    assert_eq!(runtime.key_target_for_node(children[1]), Some(children[1]));
    // A node with no release handler takes a release without complaint.
    assert!(!runtime.dispatch_key_release(children[2], 65, None, KeyModifiers::default()));
}

#[test]
fn keyboard_focus_routes_ancestors_and_cycles() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "focus.lua",
            br#"
                local ui = require("morf.ui")
                ui.Item {
                  ui.MouseArea {
                    ui.Rect {},
                    on_key_pressed = function() end,
                  },
                  ui.MouseArea {
                    focus = true,
                    on_key_pressed = function() end,
                  },
                  ui.MouseArea {
                    enabled = false,
                    on_key_pressed = function() end,
                  },
                }
                ui.Item {
                  ui.MouseArea {
                    on_key_pressed = function() end,
                  },
                }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let second_root = runtime.scene().roots()[1];
    let children = runtime.scene().children(root).unwrap().to_vec();
    let second_target = runtime.scene().children(second_root).unwrap()[0];
    let nested = runtime.scene().children(children[0]).unwrap()[0];

    assert_eq!(runtime.key_target_for_node(nested), Some(children[0]));
    assert!(runtime.node_in_subtree(root, nested));
    assert!(!runtime.node_in_subtree(second_root, nested));
    assert_eq!(runtime.first_key_target_in(root), Some(children[1]));
    assert_eq!(
        runtime.first_key_target_in(second_root),
        Some(second_target)
    );
    // Traversal is scoped to one root: Tab cycles within the surface that has
    // focus and does not wander into another surface's tree.
    assert_eq!(
        runtime.next_key_target_in(root, Some(children[1])),
        Some(children[0])
    );
    assert_eq!(
        runtime.next_key_target_in(second_root, None),
        Some(second_target)
    );
}
