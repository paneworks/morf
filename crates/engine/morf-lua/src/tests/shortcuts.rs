use super::*;
use crate::{EventPoint, KeyModifiers, UiEvent};

fn ctrl() -> KeyModifiers {
    KeyModifiers {
        ctrl: true,
        ..Default::default()
    }
}

fn setup() -> (Runtime, NodeHandle, NodeHandle, NodeHandle) {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "shortcuts.lua",
            br#"
                local ui = require("morf.ui")
                hits = {}
                local function hit(name) return function(seq) hits[#hits + 1] = name .. ":" .. seq end end
                ui.Item {
                    shortcuts = { ["ctrl+b"] = hit("root"), ["ctrl+k ctrl+s"] = hit("root") },
                    ui.Item {
                        shortcuts = { ["ctrl+b"] = hit("inner"), ["ctrl+p"] = function() return false end },
                        ui.TextInput { width = 10, height = 10 },
                    },
                    ui.Item { shortcuts = { scope = "surface", ["ctrl+p"] = hit("anywhere"), ["F5"] = hit("anywhere") } },
                }
                morf.ipc.hits = function() local s = table.concat(hits, ","); hits = {}; return s end
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let inner = runtime.scene().children(root).unwrap()[0];
    let field = runtime.scene().children(inner).unwrap()[0];
    (runtime, root, inner, field)
}

fn hits(runtime: &mut Runtime) -> String {
    match runtime.call_ipc("hits", &[]).unwrap().as_slice() {
        [IpcValue::String(s)] => s.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_nearest_shortcut_around_focus_wins() {
    let (mut runtime, root, inner, field) = setup();
    assert!(runtime.dispatch_shortcut(root, Some(field), 'b' as u32, ctrl()));
    assert!(
        runtime.dispatch_shortcut(
            root,
            Some(root),
            'B' as u32,
            KeyModifiers {
                shift: true,
                ..ctrl()
            }
        ) == false
    );
    assert!(runtime.dispatch_shortcut(root, Some(root), 'b' as u32, ctrl()));
    assert_eq!(hits(&mut runtime), "inner:ctrl+b,root:ctrl+b");
    // Returning false passes the key on, to the surface's shortcuts.
    assert!(runtime.dispatch_shortcut(root, Some(inner), 'p' as u32, ctrl()));
    assert_eq!(hits(&mut runtime), "anywhere:ctrl+p");
}

#[test]
fn a_sequence_waits_for_its_next_chord() {
    let (mut runtime, root, _, field) = setup();
    assert!(runtime.dispatch_shortcut(root, Some(field), 'k' as u32, ctrl()));
    assert_eq!(hits(&mut runtime), "");
    assert!(runtime.dispatch_shortcut(root, Some(field), 's' as u32, ctrl()));
    assert_eq!(hits(&mut runtime), "root:ctrl+k ctrl+s");
    // A key that breaks a sequence goes on alone.
    assert!(runtime.dispatch_shortcut(root, Some(field), 'k' as u32, ctrl()));
    assert!(runtime.dispatch_shortcut(root, Some(field), 'b' as u32, ctrl()));
    assert_eq!(hits(&mut runtime), "inner:ctrl+b");
}

#[test]
fn plain_keys_are_typing_in_a_field() {
    let (mut runtime, root, _, field) = setup();
    let f5 = crate::keys::keysym("F5").unwrap();
    assert!(runtime.dispatch_shortcut(root, Some(field), f5, Default::default()));
    assert!(!runtime.dispatch_shortcut(root, Some(field), 'x' as u32, Default::default()));
    assert_eq!(hits(&mut runtime), "anywhere:F5");
}

#[test]
fn a_bad_shortcut_is_an_error_where_it_is_written() {
    let mut runtime = Runtime::default();
    let error = runtime
        .execute(
            "bad.lua",
            br#"require("morf.ui").Item { shortcuts = { ["hyper+q"] = function() end } }"#,
        )
        .unwrap_err();
    assert!(error.to_string().contains("hyper"), "{error}");
}

fn gesture_setup() -> (Runtime, NodeHandle) {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "gestures.lua",
            br#"
                local ui = require("morf.ui")
                seen = {}
                ui.MouseArea { width = 200, height = 200,
                    on_clicked = function() seen[#seen + 1] = "click" end,
                    on_double_clicked = function() seen[#seen + 1] = "double" end,
                    on_long_pressed = function() seen[#seen + 1] = "long" end,
                    on_swiped = function(direction) seen[#seen + 1] = "swipe-" .. direction end,
                }
                morf.ipc.hits = function() local s = table.concat(seen, ","); seen = {}; return s end
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    (runtime, root)
}

fn at(x: f64, y: f64) -> EventPoint {
    EventPoint::new((x, y), (x, y)).with_button(0x110)
}

#[test]
fn two_quick_clicks_are_a_double_click() {
    let (mut runtime, area) = gesture_setup();
    for _ in 0..2 {
        runtime.dispatch_pointer(area, UiEvent::Pressed, at(10.0, 10.0), (0.0, 0.0));
        runtime.dispatch_pointer(area, UiEvent::Released, at(10.0, 10.0), (0.0, 0.0));
        runtime.dispatch_pointer(area, UiEvent::Clicked, at(10.0, 10.0), (0.0, 0.0));
    }
    assert_eq!(hits(&mut runtime), "click,click,double");
}

#[test]
fn a_held_press_is_a_long_press_and_takes_the_click() {
    let (mut runtime, area) = gesture_setup();
    runtime.dispatch_pointer(area, UiEvent::Pressed, at(10.0, 10.0), (0.0, 0.0));
    assert!(runtime.next_deadline().is_some());
    std::thread::sleep(std::time::Duration::from_millis(520));
    runtime.poll_services();
    runtime.dispatch_pointer(area, UiEvent::Released, at(10.0, 10.0), (0.0, 0.0));
    runtime.dispatch_pointer(area, UiEvent::Clicked, at(10.0, 10.0), (0.0, 0.0));
    assert_eq!(hits(&mut runtime), "long");
}

#[test]
fn a_fling_is_a_swipe() {
    let (mut runtime, area) = gesture_setup();
    runtime.dispatch_pointer(area, UiEvent::Pressed, at(10.0, 100.0), (0.0, 0.0));
    for step in 1..=4 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        let x = 10.0 + 30.0 * f64::from(step);
        runtime.dispatch_pointer(area, UiEvent::Dragged, at(x, 100.0), (x - 10.0, 0.0));
    }
    runtime.dispatch_pointer(area, UiEvent::Released, at(130.0, 100.0), (0.0, 0.0));
    assert_eq!(hits(&mut runtime), "swipe-right");
}
