use super::*;

#[test]
fn a_text_input_draws_its_text_clipped_with_its_caret_and_selection() {
    let mut scene = Scene::new();
    let input = scene.create(Element::TextInput);
    scene.assign(input, "width", 120.0).unwrap();
    scene.assign(input, "height", 30.0).unwrap();
    scene.assign(input, "text", "pass").unwrap();
    scene.assign(input, "password", true).unwrap();
    scene.assign(input, "focus", true).unwrap();
    scene.assign(input, "cursor_position", 3.0).unwrap();
    scene.assign(input, "selection_start", 1.0).unwrap();
    scene.assign(input, "selection_end", 3.0).unwrap();
    scene.assign(input, "scroll_x", 7.0).unwrap();
    scene.assign(input, "color", "#ff0000").unwrap();
    let layout = Layout::compute(
        &scene,
        input,
        Size {
            width: 120.0,
            height: 30.0,
        },
        &mut NoText,
    )
    .unwrap();

    let list = DrawList::from_scene(&scene, &layout).unwrap();
    let DrawCommand::Text {
        ref text,
        clip,
        color,
        vertical_alignment,
        ref edit,
        ..
    } = list.commands[0]
    else {
        panic!("a text input did not emit a text command");
    };
    // Dots, one per letter, and offsets into the dots.
    let dot = '•'.len_utf8();
    assert_eq!(text, "••••");
    assert_eq!(clip, Some(list.commands[0].bounds()));
    assert_eq!(color, Color::rgba8(255, 0, 0, 255));
    assert_eq!(vertical_alignment, VerticalAlignment::Center);
    let edit = edit.as_ref().expect("an edit");
    assert_eq!(edit.caret, Some(3 * dot));
    assert_eq!(edit.selection, dot..3 * dot);
    assert_eq!(edit.scroll, (7.0, 0.0));
    // An unset caret colour is the text's.
    assert_eq!(edit.caret_color, color);
    assert!(!edit.placeholder);

    // Without the keyboard there is no caret and no selection drawn.
    scene.assign(input, "focus", false).unwrap();
    let list = DrawList::from_scene(&scene, &layout).unwrap();
    let DrawCommand::Text { ref edit, .. } = list.commands[0] else {
        panic!("a text input did not emit a text command");
    };
    let edit = edit.as_ref().expect("an edit");
    assert_eq!(edit.caret, None);
    assert!(edit.selection.is_empty());
}

#[test]
fn an_empty_text_input_draws_its_placeholder_in_its_own_colour() {
    let mut scene = Scene::new();
    let input = scene.create(Element::TextInput);
    scene.assign(input, "placeholder", "Search").unwrap();
    scene.assign(input, "placeholder_color", "#00ff00").unwrap();
    let layout = Layout::compute(
        &scene,
        input,
        Size {
            width: 120.0,
            height: 30.0,
        },
        &mut NoText,
    )
    .unwrap();
    let list = DrawList::from_scene(&scene, &layout).unwrap();
    let DrawCommand::Text {
        ref text,
        color,
        ref edit,
        ..
    } = list.commands[0]
    else {
        panic!("a text input did not emit a text command");
    };
    assert_eq!(text, "Search");
    assert_eq!(color, Color::rgba8(0, 255, 0, 255));
    assert!(edit.as_ref().expect("an edit").placeholder);
}
