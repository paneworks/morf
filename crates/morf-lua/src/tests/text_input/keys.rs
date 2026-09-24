use super::*;

#[test]
fn typing_edits_the_text_and_says_so() {
    let (mut runtime, node) = focused("");
    type_text(&mut runtime, node, "hi");
    assert_eq!(text(&runtime, node), "hi");
    assert_eq!(number(&runtime, node, "cursor_position"), 2.0);
    assert_eq!(log(&mut runtime), "changed h|changed hi");
    press(&mut runtime, node, BACKSPACE, NONE);
    assert_eq!(text(&runtime, node), "h");
    assert_eq!(log(&mut runtime), "changed h");
}

#[test]
fn a_binding_on_text_follows_the_keyboard() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "bound.lua",
            br#"
                local ui = require("morf.ui")
                local input
                input = ui.TextInput { width = 100, height = 20 }
                ui.Item {
                    input,
                    ui.Text { text = function() return "echo " .. input.text end },
                }
            "#,
        )
        .unwrap();
    let (input, echo) = {
        let scene = runtime.scene();
        let root = scene.roots().last().copied().unwrap();
        let children = scene.children(root).unwrap().to_vec();
        (children[0], children[1])
    };
    runtime.set_key_focus(Some(input));
    type_text(&mut runtime, input, "ok");
    assert_eq!(
        runtime.scene().string_value(echo, "text").unwrap(),
        "echo ok"
    );
}

#[test]
fn arrows_words_and_shift_select() {
    let (mut runtime, node) = focused(r#"text = "one two three","#);
    assert_eq!(number(&runtime, node, "cursor_position"), 13.0);
    press(&mut runtime, node, LEFT, CTRL);
    assert_eq!(number(&runtime, node, "cursor_position"), 8.0);
    press(&mut runtime, node, LEFT, CTRL_SHIFT);
    assert_eq!(number(&runtime, node, "selection_start"), 4.0);
    assert_eq!(number(&runtime, node, "selection_end"), 8.0);
    type_text(&mut runtime, node, "2 ");
    assert_eq!(text(&runtime, node), "one 2 three");
    press(&mut runtime, node, HOME, NONE);
    assert_eq!(number(&runtime, node, "cursor_position"), 0.0);
    press(&mut runtime, node, RIGHT, SHIFT);
    press(&mut runtime, node, RIGHT, SHIFT);
    press(&mut runtime, node, END, SHIFT);
    assert_eq!(number(&runtime, node, "selection_start"), 0.0);
    assert_eq!(number(&runtime, node, "selection_end"), 11.0);
    press(&mut runtime, node, BACKSPACE, CTRL);
    assert_eq!(text(&runtime, node), "");
}

#[test]
fn word_erasing_and_select_all() {
    let (mut runtime, node) = focused(r#"text = "open the door","#);
    press(&mut runtime, node, BACKSPACE, CTRL);
    assert_eq!(text(&runtime, node), "open the ");
    letter(&mut runtime, node, 'a', CTRL);
    type_text(&mut runtime, node, "x");
    assert_eq!(text(&runtime, node), "x");
}

#[test]
fn enter_accepts_and_escape_escapes() {
    let (mut runtime, node) = focused(r#"text = "query","#);
    runtime.dispatch_key(node, RETURN, Some("\r"), NONE);
    assert_eq!(log(&mut runtime), "accepted query");
    assert_eq!(text(&runtime, node), "query");
    press(&mut runtime, node, ESCAPE, NONE);
    assert_eq!(log(&mut runtime), "escape");
}

#[test]
fn several_lines_take_enter_and_ctrl_enter_accepts() {
    let (mut runtime, node) = focused(r#"multiline = true, text = "a","#);
    runtime.dispatch_key(node, RETURN, Some("\r"), NONE);
    type_text(&mut runtime, node, "b");
    assert_eq!(text(&runtime, node), "a\nb");
    log(&mut runtime);
    runtime.dispatch_key(node, RETURN, Some("\r"), CTRL);
    assert_eq!(log(&mut runtime), "accepted a\nb");
}

#[test]
fn up_and_down_move_between_lines() {
    let (mut runtime, node) = focused(r#"multiline = true, text = "abcd\nef\nghij","#);
    // At the end of "ghij"; up lands at the end of the shorter "ef"...
    press(&mut runtime, node, UP, NONE);
    assert_eq!(number(&runtime, node, "cursor_position"), 7.0);
    // ...and up again keeps the column it started from.
    press(&mut runtime, node, UP, NONE);
    assert_eq!(number(&runtime, node, "cursor_position"), 4.0);
    press(&mut runtime, node, UP, NONE);
    assert_eq!(number(&runtime, node, "cursor_position"), 0.0);
    press(&mut runtime, node, DOWN, SHIFT);
    assert_eq!(number(&runtime, node, "selection_end"), 5.0);
}

#[test]
fn undo_and_redo_with_the_keyboard() {
    let (mut runtime, node) = focused("");
    type_text(&mut runtime, node, "hello world");
    letter(&mut runtime, node, 'z', CTRL);
    assert_eq!(text(&runtime, node), "hello");
    letter(&mut runtime, node, 'Z', CTRL_SHIFT);
    assert_eq!(text(&runtime, node), "hello world");
    letter(&mut runtime, node, 'z', CTRL);
    letter(&mut runtime, node, 'y', CTRL);
    assert_eq!(text(&runtime, node), "hello world");
}

#[test]
fn copy_cut_and_paste_use_the_clipboard() {
    let (mut runtime, node) = focused(r#"text = "copy me","#);
    letter(&mut runtime, node, 'a', CTRL);
    letter(&mut runtime, node, 'c', CTRL);
    assert_eq!(copied(&mut runtime), ["copy me"]);
    press(&mut runtime, node, END, NONE);
    letter(&mut runtime, node, 'v', CTRL);
    assert_eq!(text(&runtime, node), "copy mecopy me");
    // What the compositor says the clipboard holds is what is pasted next.
    runtime.dispatch_clipboard(Some("\nfrom elsewhere\n".to_owned()));
    letter(&mut runtime, node, 'a', CTRL);
    letter(&mut runtime, node, 'v', CTRL);
    // One line takes the breaks as spaces.
    assert_eq!(text(&runtime, node), " from elsewhere ");
    letter(&mut runtime, node, 'a', CTRL);
    letter(&mut runtime, node, 'x', CTRL);
    assert_eq!(text(&runtime, node), "");
    assert_eq!(copied(&mut runtime), [" from elsewhere "]);
}

#[test]
fn a_password_is_neither_copied_nor_cut() {
    let (mut runtime, node) = focused(r#"password = true, text = "secret","#);
    letter(&mut runtime, node, 'a', CTRL);
    letter(&mut runtime, node, 'c', CTRL);
    letter(&mut runtime, node, 'x', CTRL);
    assert!(copied(&mut runtime).is_empty());
    assert_eq!(text(&runtime, node), "secret");
}

#[test]
fn max_length_and_read_only_hold() {
    let (mut runtime, node) = focused("max_length = 3,");
    type_text(&mut runtime, node, "abcdef");
    assert_eq!(text(&runtime, node), "abc");
    let (mut runtime, node) = focused(r#"read_only = true, text = "fixed","#);
    type_text(&mut runtime, node, "x");
    press(&mut runtime, node, BACKSPACE, NONE);
    assert_eq!(text(&runtime, node), "fixed");
    // Still selectable, and copyable.
    letter(&mut runtime, node, 'a', CTRL);
    letter(&mut runtime, node, 'c', CTRL);
    assert_eq!(copied(&mut runtime), ["fixed"]);
}

#[test]
fn multibyte_text_moves_a_letter_at_a_time() {
    let (mut runtime, node) = focused(r#"text = "añ😀","#);
    press(&mut runtime, node, LEFT, NONE);
    assert_eq!(number(&runtime, node, "cursor_position"), 3.0);
    press(&mut runtime, node, BACKSPACE, NONE);
    assert_eq!(text(&runtime, node), "a😀");
    assert_eq!(number(&runtime, node, "cursor_position"), 1.0);
}

#[test]
fn keys_the_field_does_not_use_reach_its_key_handler() {
    let (mut runtime, node) = focused(
        r#"on_key_pressed = function(keysym, text, modifiers) record("key")(keysym, modifiers) end,"#,
    );
    press(&mut runtime, node, DOWN, NONE);
    let seen = log(&mut runtime);
    assert!(seen.contains(&format!("key {DOWN} ")), "{seen}");
    letter(&mut runtime, node, 'q', CTRL);
    let seen = log(&mut runtime);
    assert!(seen.contains("ctrl"), "{seen}");
    assert_eq!(text(&runtime, node), "");
}

#[test]
fn the_configuration_writes_text_caret_and_selection() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "methods.lua",
            br#"
                local ui = require("morf.ui")
                local changes = 0
                input = ui.TextInput { text = "hello world", on_text_changed = function() changes = changes + 1 end }
                morf.ipc["run"] = function()
                    input.text = "replaced"
                    assert(input.cursor_position == 8, "caret goes to the end of new text")
                    input:select(0, 3)
                    assert(input:selected_text() == "rep", "select: " .. input:selected_text())
                    assert(input.selection_start == 0 and input.selection_end == 3, "range")
                    input:insert("RE")
                    assert(input.text == "RElaced", "insert: " .. input.text)
                    input:select_all()
                    assert(input:selected_text() == "RElaced", "all: " .. input:selected_text())
                    input.cursor_position = 2
                    assert(input:selected_text() == "", "collapsed: " .. input:selected_text())
                    assert(input:undo(), "undo")
                    return input.text
                end
                morf.ipc["changes"] = function() return changes end
            "#,
        )
        .unwrap();
    assert_eq!(
        runtime.call_ipc("run", &[]).unwrap(),
        [IpcValue::String("replaced".to_owned())]
    );
    // An assignment is not announced; the insert and the undo are.
    assert_eq!(
        runtime.call_ipc("changes", &[]).unwrap(),
        [IpcValue::Integer(2)]
    );
}

#[test]
fn a_line_built_with_a_break_holds_it_as_a_space() {
    // Built with its text and a caret at the end, a single line used to keep
    // the break: shaped as two lines, it drew that caret on the empty one
    // below. The field's rules apply from the first letter, as they do to
    // any text written later, and saying so is not an edit.
    let (mut runtime, node) = field(r#"text = "books\n", cursor_position = 6,"#);
    assert_eq!(text(&runtime, node), "books ");
    assert_eq!(number(&runtime, node, "cursor_position"), 6.0);
    assert_eq!(log(&mut runtime), "");
}

#[test]
fn a_field_built_too_long_is_cut_to_its_max_length() {
    let (runtime, node) = field(r#"text = "abcdef", max_length = 3, cursor_position = 6,"#);
    assert_eq!(text(&runtime, node), "abc");
    assert_eq!(number(&runtime, node, "cursor_position"), 3.0);
}

#[test]
fn repeats_type_and_releases_reach_the_handler() {
    let (mut runtime, node) = focused(
        r#"text = "abc",
           on_key_pressed = function(keysym, text, modifiers, repeat_)
               record("key")(keysym, tostring(text), modifiers, repeat_)
           end,
           on_key_released = function(keysym, text, modifiers)
               record("up")(keysym, tostring(text), modifiers)
           end,"#,
    );
    press(&mut runtime, node, END, NONE);
    // A held Backspace keeps deleting: the field takes a repeat as a press.
    runtime.dispatch_key_press(node, BACKSPACE, None, NONE, false);
    runtime.dispatch_key_press(node, BACKSPACE, None, NONE, true);
    assert_eq!(text(&runtime, node), "a");
    log(&mut runtime);
    // What the field has no use for goes on, saying whether it repeated.
    runtime.dispatch_key_press(node, UP, None, NONE, true);
    assert_eq!(log(&mut runtime), "key 65362 nil  true");
    runtime.dispatch_key_release(node, BACKSPACE, None, NONE);
    assert_eq!(log(&mut runtime), "up 65288 nil ");
}
