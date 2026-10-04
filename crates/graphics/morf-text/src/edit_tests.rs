use super::EditBuffer;

fn at(text: &str, cursor: usize) -> EditBuffer {
    let mut buffer = EditBuffer::new(text);
    buffer.set_cursor(cursor, false);
    buffer
}

#[test]
fn typing_inserts_at_the_caret_and_moves_it_on() {
    let mut buffer = at("helo", 3);
    assert!(buffer.insert("l"));
    assert_eq!(buffer.text(), "hello");
    assert_eq!(buffer.cursor(), 4);
}

#[test]
fn typing_replaces_the_selection() {
    let mut buffer = EditBuffer::new("hello world");
    buffer.select(0, 5);
    assert_eq!(buffer.selected_text(), "hello");
    buffer.insert("goodbye");
    assert_eq!(buffer.text(), "goodbye world");
    assert_eq!(buffer.cursor(), 7);
    assert!(!buffer.has_selection());
}

#[test]
fn backspace_and_delete_take_whole_graphemes() {
    // "é" as e and a combining acute: two code points, three bytes, one letter.
    let text = "ae\u{301}b";
    let mut buffer = at(text, 4);
    assert!(buffer.backspace(false));
    assert_eq!(buffer.text(), "ab");
    assert_eq!(buffer.cursor(), 1);

    let mut buffer = at(text, 1);
    assert!(buffer.delete(false));
    assert_eq!(buffer.text(), "ab");
    assert_eq!(buffer.cursor(), 1);
}

#[test]
fn emoji_with_modifiers_are_one_step() {
    // A family: four people joined by zero-width joiners.
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    let text = format!("a{family}b");
    let mut buffer = at(&text, 1);
    buffer.move_right(false, false);
    assert_eq!(buffer.cursor(), 1 + family.len());
    buffer.move_left(false, false);
    assert_eq!(buffer.cursor(), 1);
    buffer.set_cursor(1 + family.len(), false);
    buffer.backspace(false);
    assert_eq!(buffer.text(), "ab");
}

#[test]
fn an_offset_inside_a_letter_falls_back_to_its_start() {
    let mut buffer = EditBuffer::new("añb");
    // Byte 2 is the middle of "ñ".
    buffer.set_cursor(2, false);
    assert_eq!(buffer.cursor(), 1);
    buffer.set_cursor(99, false);
    assert_eq!(buffer.cursor(), buffer.text().len());
}

#[test]
fn backspace_at_the_start_and_delete_at_the_end_do_nothing() {
    let mut buffer = at("ab", 0);
    assert!(!buffer.backspace(false));
    buffer.move_end(false);
    assert!(!buffer.delete(false));
    assert_eq!(buffer.text(), "ab");
}

#[test]
fn word_motion_skips_space_and_punctuation() {
    let mut buffer = at("one, two  three", 15);
    buffer.move_left(true, false);
    assert_eq!(buffer.cursor(), 10);
    buffer.move_left(true, false);
    assert_eq!(buffer.cursor(), 5);
    buffer.move_left(true, false);
    assert_eq!(buffer.cursor(), 0);
    buffer.move_right(true, false);
    assert_eq!(buffer.cursor(), 3);
    buffer.move_right(true, false);
    assert_eq!(buffer.cursor(), 8);
    buffer.move_right(true, false);
    assert_eq!(buffer.cursor(), 15);
}

#[test]
fn word_erasing_takes_the_word_before_or_after() {
    let mut buffer = at("open the door", 8);
    assert!(buffer.backspace(true));
    assert_eq!(buffer.text(), "open  door");
    assert_eq!(buffer.cursor(), 5);
    assert!(buffer.delete(true));
    assert_eq!(buffer.text(), "open ");
}

#[test]
fn shift_extends_the_selection_from_the_anchor() {
    let mut buffer = at("hello", 1);
    buffer.move_right(false, true);
    buffer.move_right(false, true);
    assert_eq!(buffer.selection(), 1..3);
    assert_eq!(buffer.selected_text(), "el");
    buffer.move_left(true, true);
    assert_eq!(buffer.selection(), 0..1);
    assert_eq!(buffer.anchor(), 1);
}

#[test]
fn an_arrow_without_shift_collapses_the_selection_to_its_side() {
    let mut buffer = EditBuffer::new("hello");
    buffer.select(1, 4);
    buffer.move_left(false, false);
    assert_eq!(buffer.cursor(), 1);
    assert!(!buffer.has_selection());
    buffer.select(1, 4);
    buffer.move_right(false, false);
    assert_eq!(buffer.cursor(), 4);
}

#[test]
fn select_all_then_type_replaces_everything() {
    let mut buffer = EditBuffer::new("old");
    buffer.select_all();
    buffer.insert("new");
    assert_eq!(buffer.text(), "new");
}

#[test]
fn home_and_end_stay_on_the_line_of_text() {
    let mut buffer = EditBuffer::new("first\nsecond\nthird");
    buffer.multiline = true;
    buffer.set_cursor(9, false);
    buffer.move_line_start(false);
    assert_eq!(buffer.cursor(), 6);
    buffer.move_line_end(true);
    assert_eq!(buffer.cursor(), 12);
    assert_eq!(buffer.selected_text(), "second");
    buffer.move_start(false);
    assert_eq!(buffer.cursor(), 0);
    buffer.move_end(false);
    assert_eq!(buffer.cursor(), buffer.text().len());
}

#[test]
fn a_single_line_turns_pasted_breaks_into_spaces() {
    let mut buffer = EditBuffer::new("");
    buffer.insert("one\ntwo\r\nthree");
    assert_eq!(buffer.text(), "one two three");
}

#[test]
fn several_lines_keep_breaks_and_normalise_carriage_returns() {
    let mut buffer = EditBuffer::new("");
    buffer.multiline = true;
    buffer.insert("one\r\ntwo\rthree");
    assert_eq!(buffer.text(), "one\ntwo\nthree");
}

#[test]
fn control_characters_are_not_text() {
    let mut buffer = EditBuffer::new("");
    assert!(!buffer.insert("\u{1b}"));
    assert!(!buffer.insert("\u{8}"));
    assert_eq!(buffer.text(), "");
}

#[test]
fn max_length_counts_characters_and_drops_what_does_not_fit() {
    let mut buffer = EditBuffer::new("");
    buffer.max_length = 4;
    buffer.insert("añbcd");
    assert_eq!(buffer.text(), "añbc");
    assert!(!buffer.insert("x"));
    // Replacing a selection makes room for what replaces it.
    // "añ" is three bytes.
    buffer.select(0, 3);
    assert!(buffer.insert("xy"));
    assert_eq!(buffer.text(), "xybc");
}

#[test]
fn undo_takes_back_a_run_of_typing_a_word_at_a_time() {
    let mut buffer = EditBuffer::new("");
    for character in "hello world".chars() {
        buffer.insert(&character.to_string());
    }
    assert!(buffer.undo());
    assert_eq!(buffer.text(), "hello");
    assert!(buffer.undo());
    assert_eq!(buffer.text(), "");
    assert!(!buffer.undo());
    assert!(buffer.redo());
    assert_eq!(buffer.text(), "hello");
    assert!(buffer.redo());
    assert_eq!(buffer.text(), "hello world");
    assert!(!buffer.redo());
}

#[test]
fn moving_the_caret_ends_a_run() {
    let mut buffer = EditBuffer::new("");
    buffer.insert("a");
    buffer.insert("b");
    buffer.move_left(false, false);
    buffer.insert("c");
    assert_eq!(buffer.text(), "acb");
    buffer.undo();
    assert_eq!(buffer.text(), "ab");
    assert_eq!(buffer.cursor(), 1);
    buffer.undo();
    assert_eq!(buffer.text(), "");
}

#[test]
fn a_new_edit_forgets_what_was_undone() {
    let mut buffer = EditBuffer::new("");
    buffer.insert("a");
    buffer.undo();
    buffer.insert("b");
    assert!(!buffer.redo());
    assert_eq!(buffer.text(), "b");
}

#[test]
fn undo_restores_a_deleted_selection_and_selects_it_again() {
    let mut buffer = EditBuffer::new("keep this");
    buffer.select(4, 9);
    buffer.backspace(false);
    assert_eq!(buffer.text(), "keep");
    buffer.undo();
    assert_eq!(buffer.text(), "keep this");
    assert_eq!(buffer.selection(), 4..9);
}

#[test]
fn backspaces_in_a_row_undo_together() {
    let mut buffer = EditBuffer::new("abcdef");
    buffer.backspace(false);
    buffer.backspace(false);
    buffer.backspace(false);
    assert_eq!(buffer.text(), "abc");
    buffer.undo();
    assert_eq!(buffer.text(), "abcdef");
}

#[test]
fn history_is_bounded() {
    let mut buffer = EditBuffer::new("");
    buffer.set_history_limit(3);
    for word in ["a ", "b ", "c ", "d ", "e "] {
        buffer.insert(word);
    }
    let mut undone = 0;
    while buffer.undo() {
        undone += 1;
    }
    assert_eq!(undone, 3);
    assert_eq!(buffer.text(), "a b ");
}

#[test]
fn set_text_replaces_everything_and_forgets_history() {
    let mut buffer = EditBuffer::new("");
    buffer.insert("typed");
    buffer.set_text("from outside");
    assert_eq!(buffer.cursor(), "from outside".len());
    assert!(!buffer.can_undo());
}

#[test]
fn a_double_click_selects_the_word_under_it() {
    let buffer = EditBuffer::new("say hello, world");
    assert_eq!(buffer.word_at(6), 4..9);
    // At a word's start, and just past its end, it is still that word.
    assert_eq!(buffer.word_at(4), 4..9);
    assert_eq!(buffer.word_at(9), 4..9);
    // Between two spaces is the space.
    let buffer = EditBuffer::new("a   b");
    assert_eq!(buffer.word_at(2), 1..4);
    // Past the end is the last word.
    let buffer = EditBuffer::new("end");
    assert_eq!(buffer.word_at(3), 0..3);
}

#[test]
fn words_in_scripts_without_spaces_are_still_graphemes_apart() {
    let mut buffer = at("日本語", 0);
    buffer.move_right(false, false);
    assert_eq!(buffer.cursor(), 3);
    buffer.move_right(false, true);
    assert_eq!(buffer.selected_text(), "本");
}

#[test]
fn replace_is_one_undoable_edit() {
    let mut buffer = EditBuffer::new("teh cat");
    assert!(buffer.replace(0..3, "the"));
    assert_eq!(buffer.text(), "the cat");
    assert_eq!(buffer.cursor(), 3);
    buffer.undo();
    assert_eq!(buffer.text(), "teh cat");
}
