//! The properties of the text elements: `ui.Text` and `ui.TextInput`.

use super::*;

/// What `ui.Text` adds to every element's properties.
pub(super) fn text() -> Vec<PropertySpec> {
    vec![
        string("text", ""),
        // Styled runs, which win over `text`; `markup` wins over both.
        any("spans", Value::List(Vec::new())),
        string("markup", ""),
        // The colour of a link run that names none; nil is the text's.
        any("link_color", Value::Nil),
        // Kept by the runtime: where each link run was laid out, as
        // `{ href, x, y, width, height }` in the node's own space.
        any("links", Value::List(Vec::new())),
        // The pointer's shape over a link.
        string("cursor", "pointer"),
        color("color", Color::rgba8(0, 0, 0, 255)),
        number("font_size", 16.0),
        number("font_weight", 400.0),
        string("font_family", "sans-serif"),
        string("font_source", ""),
        // A multiple of the font size, or a `px` size as a string.
        any("line_height", Value::Number(1.2)),
        number("letter_spacing", 0.0),
        number("word_spacing", 0.0),
        string("font_style", "normal"),
        string("font_stretch", "normal"),
        // A variable font's axes by their four-letter tags:
        // `{ FILL = 1, GRAD = 0, opsz = 24, wght = 500 }`. A map of
        // numbers, so a behavior moves it like any other number.
        any("axes", Value::Map(BTreeMap::new())),
        // `auto` sets a face's `opsz` axis to the font size unless
        // `axes` names it, as CSS does; `none` leaves it at its default.
        string("optical_sizing", "auto"),
        // `{ line, thickness, offset, color }`; empty is none.
        any("decoration", Value::Map(BTreeMap::new())),
        boolean("wrap", false),
        string("elide", "none"),
        // Wrapped text stops after this many lines, the last one
        // elided. Zero is no limit.
        number("max_lines", 0.0),
        string("horizontal_alignment", "left"),
        string("vertical_alignment", "top"),
        // Glyphs are distance fields, so the edge is a threshold rather
        // than a set of pixels: these move it, soften it, and read a
        // second one further out as an outline. All ordinary numbers,
        // so all animatable, which is the reason for storing letters
        // this way at all.
        number("thickness", 0.0),
        number("softness", 0.0),
        number("outline_width", 0.0),
        color("outline_color", Color::rgba8(0, 0, 0, 0)),
        // The text this one turns into, and how far along it is.
        //
        // Not a crossfade between two labels: the glyphs are distance
        // fields, so the two are interpolated as fields and thresholded
        // once, and the outline travels from one letter's shape to the
        // other's through shapes that belong to neither. Glyphs pair up
        // by position, and one with nothing opposite it dissolves.
        string("morph_to", ""),
        number("morph_progress", 0.0),
    ]
}

/// What `ui.TextInput` adds to every element's properties.
pub(super) fn text_input() -> Vec<PropertySpec> {
    vec![
        // What the field holds. Written by the keyboard as much as by
        // the configuration: an edit assigns it, which is what a
        // binding on it hears.
        string("text", ""),
        // Shown, in its own colour, while `text` is empty.
        string("placeholder", ""),
        color("placeholder_color", Color::rgba8(0, 0, 0, 102)),
        color("color", Color::rgba8(0, 0, 0, 255)),
        // The same type vocabulary `Text` has, because it is the same
        // shaper underneath.
        number("font_size", 16.0),
        number("font_weight", 400.0),
        string("font_family", "sans-serif"),
        string("font_source", ""),
        any("line_height", Value::Number(1.2)),
        number("letter_spacing", 0.0),
        number("word_spacing", 0.0),
        string("font_style", "normal"),
        string("font_stretch", "normal"),
        any("axes", Value::Map(BTreeMap::new())),
        string("optical_sizing", "auto"),
        string("horizontal_alignment", "left"),
        // Where a single line sits in a box taller than it. Several
        // lines always start at the top and scroll.
        string("vertical_alignment", "center"),
        // Enter inserts a line rather than accepting, and up and down
        // move between lines.
        boolean("multiline", false),
        // Whether several lines break at the width. A single line
        // never wraps; it scrolls sideways under the caret instead.
        boolean("wrap", true),
        // Each character is drawn as `password_char`; copying and
        // cutting are refused so the text cannot leave the field.
        boolean("password", false),
        string("password_char", "•"),
        // The most characters the field takes. Zero is no limit.
        number("max_length", 0.0),
        // Selectable and copyable, but not editable.
        boolean("read_only", false),
        // Coloured runs over the text, for a code editor's syntax:
        // `{ start, stop, color, underline, strike }`, byte offsets.
        any("highlights", Value::List(Vec::new())),
        color("selection_color", Color::rgba8(53, 132, 228, 90)),
        // Fully transparent keeps the text its own colour.
        color("selected_text_color", Color::rgba8(0, 0, 0, 0)),
        // Fully transparent means the text colour, as a layer's fill
        // does in a field: one colour to change, not two.
        color("caret_color", Color::rgba8(0, 0, 0, 0)),
        number("caret_width", 2.0),
        // Milliseconds each half of a blink lasts; zero holds still.
        number("caret_blink_interval", 530.0),
        // Where the caret is and where the selection started, as byte
        // offsets into `text` — the number of bytes before them, so
        // `text:sub(1, cursor_position)` is what is left of the caret.
        // Both may be written to move them.
        number("cursor_position", 0.0),
        number("selection_start", 0.0),
        number("selection_end", 0.0),
        // How far the content has scrolled under the box, and how big
        // it is: kept by the field so the caret stays in view, and
        // readable so a `Flickable` or a scroll bar can follow.
        number("scroll_x", 0.0),
        number("scroll_y", 0.0),
        number("content_width", 0.0),
        number("content_height", 0.0),
        // The blink's current half; the field writes it.
        boolean("caret_visible", true),
        // The pointer's shape over the field: the text beam, as any
        // other field has it.
        string("cursor", "text"),
    ]
}
