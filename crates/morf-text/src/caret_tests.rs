use morf_layout::{TextAlignment, TextMeasurer, TextOptions};
use morf_scene::{Element, Scene};

use crate::{CaretMap, TextSystem};

fn shaped(text: &str, options: TextOptions) -> (TextSystem, CaretMap) {
    let mut scene = Scene::new();
    let node = scene.create(Element::Text);
    let mut system = TextSystem::new();
    system.measure(node, text, "monospace", 20.0, options);
    let map = system.caret_map(node).expect("the node was shaped");
    (system, map)
}

#[test]
fn a_caret_moves_right_letter_by_letter() {
    let (_, map) = shaped("abcd", TextOptions::default());
    let xs = (0..=4)
        .map(|offset| map.caret(offset).x)
        .collect::<Vec<_>>();
    assert_eq!(xs[0], 0.0);
    for pair in xs.windows(2) {
        assert!(pair[1] > pair[0], "{xs:?}");
    }
    // Monospace: every letter the same advance.
    let advance = xs[1] - xs[0];
    assert!((xs[4] - 4.0 * advance).abs() < 0.5, "{xs:?}");
    let caret = map.caret(2);
    assert_eq!(caret.y, 0.0);
    assert!(caret.height >= 20.0);
}

#[test]
fn a_point_lands_on_the_nearest_boundary() {
    let (_, map) = shaped("abcd", TextOptions::default());
    let advance = map.caret(1).x;
    assert_eq!(map.index_at(-50.0, 5.0), 0);
    assert_eq!(map.index_at(advance * 0.4, 5.0), 0);
    assert_eq!(map.index_at(advance * 0.6, 5.0), 1);
    assert_eq!(map.index_at(advance * 2.9, 5.0), 3);
    assert_eq!(map.index_at(1_000.0, 5.0), 4);
    // Below the text is its last line; above it, its first.
    assert_eq!(map.index_at(1_000.0, 500.0), 4);
    assert_eq!(map.index_at(0.0, -500.0), 0);
}

#[test]
fn multibyte_letters_have_their_stops_at_their_boundaries() {
    let (_, map) = shaped("añb", TextOptions::default());
    // "ñ" runs from byte 1 to byte 3.
    let one = map.caret(1).x;
    let three = map.caret(3).x;
    assert!(three > one);
    // An offset inside the letter reads the stop before it.
    assert_eq!(map.caret(2).x, one);
    assert_eq!(map.index_at(three + 0.1, 5.0), 3);
}

#[test]
fn line_breaks_start_new_lines_with_their_own_offsets() {
    let (_, map) = shaped("ab\n\ncd", TextOptions::default());
    assert_eq!(map.lines().len(), 3);
    assert_eq!(map.line_of(2), 0);
    // The empty line between is somewhere to stand.
    assert_eq!(map.line_of(3), 1);
    assert_eq!(map.caret(3).x, 0.0);
    assert_eq!(map.line_of(4), 2);
    let second = map.caret(3);
    let third = map.caret(4);
    assert!(third.y > second.y && second.y > 0.0);
    assert_eq!(map.caret(4).x, 0.0);
    assert!(map.caret(6).x > 0.0);
    // Clicking the empty line lands on it.
    assert_eq!(map.index_at(40.0, second.y + 1.0), 3);
}

#[test]
fn a_trailing_break_leaves_an_empty_last_line() {
    let (_, map) = shaped("ab\n", TextOptions::default());
    assert_eq!(map.lines().len(), 2);
    assert_eq!(map.line_of(3), 1);
    assert_eq!(map.caret(3).x, 0.0);
    assert!(map.caret(3).y > 0.0);
}

#[test]
fn empty_text_still_has_a_caret() {
    let (_, map) = shaped("", TextOptions::default());
    let caret = map.caret(0);
    assert_eq!(caret.x, 0.0);
    assert!(caret.height > 0.0);
    assert_eq!(map.index_at(30.0, 30.0), 0);
}

#[test]
fn up_and_down_keep_to_the_column() {
    let (_, map) = shaped("abcd\nefgh\nij", TextOptions::default());
    let x = map.caret(2).x;
    assert_eq!(map.vertical(2, 1, x), Some(7));
    // A short line takes the nearest it has.
    assert_eq!(map.vertical(7, 1, map.caret(9).x), Some(12));
    assert_eq!(map.vertical(7, -1, x), Some(2));
    assert_eq!(map.vertical(2, -1, x), None);
    assert_eq!(map.vertical(11, 1, x), None);
}

#[test]
fn wrapped_lines_split_the_text_and_home_and_end_keep_to_them() {
    let options = TextOptions {
        width: Some(80.0),
        wrap: true,
        ..TextOptions::default()
    };
    let text = "aaaa bbbb cccc";
    let (_, map) = shaped(text, options);
    assert!(map.lines().len() >= 2, "{:?}", map.lines());
    let second = &map.lines()[1];
    assert!(!map.lines()[0].hard_end);
    // The offset where the lines meet is drawn at the start of the second.
    assert_eq!(map.line_of(second.start), 1);
    assert_eq!(map.caret(second.start).x, 0.0);
    // End on the first line stays on it.
    let (start, end) = map.line_bounds(2);
    assert_eq!(start, 0);
    assert!(end < second.start);
    assert_eq!(map.line_of(end), 0);
}

#[test]
fn a_selection_is_a_rectangle_per_line() {
    let (_, map) = shaped("abcd\nefgh", TextOptions::default());
    let rects = map.selection(2, 7);
    assert_eq!(rects.len(), 2);
    assert_eq!(rects[0].x, map.caret(2).x);
    // The first line's rectangle runs past its end: the break is selected.
    assert!(rects[0].x + rects[0].width > map.caret(4).x);
    assert_eq!(rects[1].x, 0.0);
    assert!((rects[1].width - map.caret(7).x).abs() < 0.01);
    assert!(rects[1].y > rects[0].y);
    assert!(map.selection(3, 3).is_empty());
    // Order does not matter.
    assert_eq!(map.selection(7, 2), rects);
}

#[test]
fn centred_text_puts_an_empty_line_in_the_middle() {
    let options = TextOptions {
        width: Some(200.0),
        alignment: TextAlignment::Center,
        ..TextOptions::default()
    };
    let (_, map) = shaped("", options);
    assert_eq!(map.caret(0).x, 100.0);
}

#[test]
fn word_spacing_moves_the_caret_with_the_word() {
    let plain = shaped("a b", TextOptions::default()).1;
    let mut options = TextOptions::default();
    options.style.word_spacing = 10.0;
    let spaced = shaped("a b", options).1;
    assert!((spaced.caret(2).x - plain.caret(2).x - 10.0).abs() < 0.5);
    assert_eq!(spaced.caret(1).x, plain.caret(1).x);
}

#[test]
fn glyphs_know_the_offsets_they_start_at() {
    let mut scene = Scene::new();
    let node = scene.create(Element::Text);
    let mut system = TextSystem::new();
    system.measure(node, "ab\ncd", "monospace", 20.0, TextOptions::default());
    let offsets = system
        .rasterize_at(node, (0.0, 0.0), 1.0)
        .into_iter()
        .map(|(_, offset)| offset)
        .collect::<Vec<_>>();
    assert_eq!(offsets, vec![0, 1, 3, 4]);
}

#[test]
fn a_uniform_map_is_a_grid() {
    let map = CaretMap::uniform("ab\ncde", 10.0, 20.0);
    assert_eq!(map.caret(1).x, 10.0);
    assert_eq!(map.caret(3).y, 20.0);
    assert_eq!(map.caret(6).x, 30.0);
    assert_eq!(map.index_at(24.0, 25.0), 5);
    assert_eq!(map.extent(), (0.0, 30.0, 40.0));
}
