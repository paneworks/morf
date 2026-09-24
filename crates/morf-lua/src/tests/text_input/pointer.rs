use super::*;

#[test]
fn one_field_has_the_keyboard_at_a_time() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "two.lua",
            br#"
                local ui = require("morf.ui")
                seen = {}
                ui.Item {
                    ui.TextInput { on_focus_changed = function(f) seen[#seen + 1] = "a" .. tostring(f) end },
                    ui.TextInput { on_focus_changed = function(f) seen[#seen + 1] = "b" .. tostring(f) end },
                    ui.MouseArea { on_key_pressed = function() end },
                }
                morf.ipc["seen"] = function() local s = table.concat(seen, ","); seen = {}; return s end
            "#,
        )
        .unwrap();
    let (root, a, b, area) = {
        let scene = runtime.scene();
        let root = scene.roots()[0];
        let children = scene.children(root).unwrap().to_vec();
        (root, children[0], children[1], children[2])
    };
    let seen = |runtime: &mut Runtime| match runtime.call_ipc("seen", &[]).unwrap().as_slice() {
        [IpcValue::String(value)] => value.clone(),
        other => panic!("{other:?}"),
    };
    runtime.set_key_focus(Some(a));
    assert_eq!(runtime.focused_text_input_in(root), Some(a));
    runtime.set_key_focus(Some(b));
    assert_eq!(seen(&mut runtime), "atrue,afalse,btrue");
    assert!(!runtime.scene().bool_value(a, "focus").unwrap());
    // A key handler taking the keyboard takes it from the field.
    runtime.set_key_focus(Some(area));
    assert_eq!(seen(&mut runtime), "bfalse");
    assert_eq!(runtime.focused_text_input_in(root), None);
    // Tab reaches text inputs as it reaches key handlers.
    assert_eq!(runtime.next_key_target_in(root, Some(a)), Some(b));
    // Writing `focus` from the configuration moves it too.
    runtime
        .scene_mut()
        .assign(a, "focus", morf_scene::Value::Bool(true))
        .unwrap();
    assert_eq!(runtime.focused_text_input_in(root), Some(a));
    assert_eq!(seen(&mut runtime), "atrue");
}

#[test]
fn clicks_place_the_caret_and_double_clicks_select_a_word() {
    // Without a renderer the field reads its text as a grid of even
    // advances: font size ten is six pixels a letter.
    let (mut runtime, node) = field(r#"text = "say hello world","#);
    let at = |x: f64| EventPoint::new((x, 5.0), (x, 5.0));
    assert!(runtime.dispatch_pointer(node, UiEvent::Pressed, at(25.0), (0.0, 0.0)));
    assert!(runtime.scene().bool_value(node, "focus").unwrap());
    assert_eq!(number(&runtime, node, "cursor_position"), 4.0);
    runtime.dispatch_pointer(node, UiEvent::Released, at(25.0), (0.0, 0.0));
    runtime.dispatch_pointer(node, UiEvent::Pressed, at(25.0), (0.0, 0.0));
    assert_eq!(number(&runtime, node, "selection_start"), 4.0);
    assert_eq!(number(&runtime, node, "selection_end"), 9.0);
    runtime.dispatch_pointer(node, UiEvent::Released, at(25.0), (0.0, 0.0));
    // A fresh press elsewhere, then a drag, selects what it covers.
    std::thread::sleep(std::time::Duration::from_millis(450));
    runtime.dispatch_pointer(node, UiEvent::Pressed, at(1.0), (0.0, 0.0));
    runtime.dispatch_pointer(node, UiEvent::Dragged, at(18.0), (17.0, 0.0));
    assert_eq!(number(&runtime, node, "selection_start"), 0.0);
    assert_eq!(number(&runtime, node, "selection_end"), 3.0);
    runtime.dispatch_pointer(node, UiEvent::Released, at(18.0), (0.0, 0.0));
    // Once let go, moving the pointer selects nothing more.
    runtime.dispatch_pointer(node, UiEvent::PointerMoved, at(60.0), (0.0, 0.0));
    assert_eq!(number(&runtime, node, "selection_end"), 3.0);
}

#[test]
fn a_text_input_takes_the_primary_button_and_is_a_key_target() {
    let (runtime, node) = field("");
    assert!(runtime.accepts_pointer_button(node, 0x110));
    assert!(!runtime.accepts_pointer_button(node, 0x111));
    assert_eq!(runtime.key_target_for_node(node), Some(node));
}

#[test]
fn a_frame_scrolls_the_caret_into_view() {
    let (mut runtime, node) = focused(&format!(r#"text = "{}","#, "wide ".repeat(40)));
    let mut text_system = morf_text::TextSystem::new();
    let layout = runtime
        .compute_layout(
            node,
            morf_layout::Size {
                width: 400.0,
                height: 100.0,
            },
            &mut text_system,
        )
        .unwrap();
    runtime.sync_text_inputs(&layout, &mut text_system);
    let content = number(&runtime, node, "content_width");
    let scroll = number(&runtime, node, "scroll_x");
    // The caret is at the end of a line far wider than the box.
    assert!(content > 200.0, "{content}");
    assert!(
        scroll > 0.0 && scroll <= content - 200.0 + 0.5,
        "{scroll} of {content}"
    );
    // Home takes the view back to the start.
    press(&mut runtime, node, HOME, NONE);
    runtime.sync_text_inputs(&layout, &mut text_system);
    assert_eq!(number(&runtime, node, "scroll_x"), 0.0);
    // A click now lands on the real glyphs: past everything is the end.
    runtime.dispatch_pointer(
        node,
        UiEvent::Pressed,
        EventPoint::new((199.0, 15.0), (199.0, 15.0)),
        (0.0, 0.0),
    );
    let caret = number(&runtime, node, "cursor_position");
    assert!(caret > 0.0 && caret < 200.0, "{caret}");
}

#[test]
fn several_lines_publish_their_height_and_scroll_down() {
    let (mut runtime, node) = focused(&format!(
        r#"multiline = true, text = "{}","#,
        "line\\n".repeat(20)
    ));
    let mut text_system = morf_text::TextSystem::new();
    let layout = runtime
        .compute_layout(
            node,
            morf_layout::Size {
                width: 400.0,
                height: 100.0,
            },
            &mut text_system,
        )
        .unwrap();
    runtime.sync_text_inputs(&layout, &mut text_system);
    let content = number(&runtime, node, "content_height");
    assert!(content > 30.0 * 5.0, "{content}");
    let height = layout.geometry(node).unwrap().height;
    let scroll = number(&runtime, node, "scroll_y");
    // Scrolled to the bottom, where the caret is.
    assert!(
        (scroll - (content - height)).abs() < 1.0,
        "{scroll} of {content} in {height}"
    );
}

#[test]
fn the_example_types_into_the_field_it_focuses() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "examples/text-input.lua",
            include_bytes!("../../../../../examples/text-input.lua"),
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let search = runtime
        .focused_text_input_in(root)
        .expect("the search box asks for the keyboard");
    type_text(&mut runtime, search, "fox");
    assert_eq!(text(&runtime, search), "fox");
    press(&mut runtime, search, ESCAPE, NONE);
    assert_eq!(text(&runtime, search), "");
}
