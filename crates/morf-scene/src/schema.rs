use std::collections::BTreeMap;

use crate::{animation::*, types::*};

pub(crate) use crate::coerce::coerce;

pub(crate) fn schema(element: Element) -> Vec<PropertySpec> {
    let mut properties = vec![
        number("x", 0.0),
        number("y", 0.0),
        number("width", 0.0),
        number("height", 0.0),
        number("implicit_width", 0.0),
        number("implicit_height", 0.0),
        any("anchors", Value::Map(BTreeMap::new())),
        boolean("visible", true),
        number("opacity", 1.0),
        any("layer", Value::Map(BTreeMap::new())),
        color("color_overlay", Color::rgba8(0, 0, 0, 0)),
        number("z", 0.0),
        boolean("clip", element == Element::ClipRect),
        // Whether the compositor should blur what is behind this node.
        //
        // On every element rather than only the drawn ones, because what it
        // marks is an area of the surface, not a way of painting: an `Item`
        // wrapping a panel is often the honest place to say it.
        boolean("backdrop_blur", false),
        number("rotation", 0.0),
        number("scale", 1.0),
        number("scale_x", 1.0),
        number("scale_y", 1.0),
        number("skew_x", 0.0),
        number("skew_y", 0.0),
        number("translate_x", 0.0),
        number("translate_y", 0.0),
        number("transform_origin_x", 0.5),
        number("transform_origin_y", 0.5),
        number("transition_x", 0.0),
        number("transition_y", 0.0),
        boolean("enabled", true),
        boolean("focus", false),
        // Whether Tab moves the keyboard away from this node while it has
        // it. False hands Tab to its own `on_key_pressed`. A terminal keeps
        // it: Tab is how a shell completes.
        boolean("tab_navigation", element != Element::Terminal),
        any("layout", Value::Map(BTreeMap::new())),
    ];
    match element {
        // Nothing of its own to paint, but a colour for the text beneath it
        // to inherit.
        Element::Item => properties.push(any("color", Value::Nil)),
        Element::Inset => properties.extend([
            number("margin", 0.0),
            number("extra_margin", 0.0),
            any("top_margin", Value::Nil),
            any("right_margin", Value::Nil),
            any("bottom_margin", Value::Nil),
            any("left_margin", Value::Nil),
            boolean("resize_child", true),
        ]),
        Element::Loader => properties.extend([
            boolean("active", true),
            boolean("loading", false),
            boolean("active_async", false),
        ]),
        Element::Timer => {
            properties.extend([
                number("interval", 1_000.0),
                boolean("repeat", false),
                boolean("running", false),
            ]);
        }
        Element::MouseArea => {
            properties.push(any(
                "accepted_buttons",
                Value::List(vec![Value::String("left".to_owned())]),
            ));
            // The pointer's shape while it is over this area.
            properties.push(string("cursor", "default"));
            // Kept by the runtime, for bindings to follow: whether the
            // pointer is over the area, and whether a button is held on it.
            // Read-only to a configuration.
            properties.push(boolean("hovered", false));
            properties.push(boolean("pressed", false));
        }
        // The types a drop here may carry, best first: exact types, `major/*`,
        // `*`, or the shorthands `text`, `image`, `uris`/`files`. Empty takes
        // anything.
        Element::DropArea => properties.push(any("keys", Value::List(Vec::new()))),
        Element::Flickable => {
            properties.extend([
                // Only the offsets. `content_width`/`content_height` were
                // declared beside them and read by nothing — not by layout, not
                // by paint, not by any configuration — so they were two
                // properties a config could set and watch do nothing.
                number("content_x", 0.0),
                number("content_y", 0.0),
            ]);
        }
        Element::Rect | Element::ClipRect => {
            properties.extend([
                color("color", Color::rgba8(255, 255, 255, 255)),
                any("gradient", Value::Map(BTreeMap::new())),
                number("radius", 0.0),
                number("top_left_radius", -1.0),
                number("top_right_radius", -1.0),
                number("bottom_right_radius", -1.0),
                number("bottom_left_radius", -1.0),
                number("border_width", 0.0),
                color("border_color", Color::rgba8(0, 0, 0, 0)),
                number("blur", 0.0),
                color("shadow_color", Color::rgba8(0, 0, 0, 0)),
                number("shadow_blur", 0.0),
                number("shadow_spread", 0.0),
                number("shadow_offset_x", 0.0),
                number("shadow_offset_y", 0.0),
                boolean("shadow_inner", false),
            ]);
            if element == Element::ClipRect {
                properties.extend([
                    boolean("content_inside_border", true),
                    boolean("content_under_border", false),
                    boolean("antialiasing", true),
                    boolean("border_pixel_aligned", true),
                ]);
            }
        }
        Element::Text => {
            properties.extend([
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
            ]);
        }
        Element::TextInput => {
            properties.extend([
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
            ]);
        }
        Element::Image => {
            properties.extend([
                string("source", ""),
                string("fill_mode", "stretch"),
                // Filtered when scaled, as a photograph wants; false samples
                // the nearest pixel, as pixel art and a magnifier want.
                boolean("smooth", true),
                number("source_width", 0.0),
                number("source_height", 0.0),
                boolean("distance_field", false),
                number("distance_field_spread", 8.0),
                // The same four names Text uses, because they are the same
                // four numbers. They were spelled `distance_field_*` here and
                // plainly there, and `weight` even meant a different thing in
                // each — an absolute threshold on one side and a signed offset
                // on the other.
                number("thickness", 0.0),
                number("softness", 0.0),
                number("outline_width", 0.0),
                color("outline_color", Color::rgba8(0, 0, 0, 0)),
                // Kept by the runtime, for bindings to follow: what became of
                // the source. `none` without one, `loading` until it was
                // looked at, then `ready` or `error` with the reason.
                string("status", "none"),
                any("error", Value::Nil),
                // A moving picture's playback. `frame` is where it is, and a
                // write seeks; `frame_count` is the runtime's.
                boolean("playing", true),
                number("speed", 1.0),
                number("frame", 0.0),
                any("loops", Value::String("forever".to_owned())),
                number("frame_count", 0.0),
            ]);
        }
        Element::Icon => {
            properties.extend([
                string("name", ""),
                string("theme", "hicolor"),
                string("fill_mode", "stretch"),
                number("source_width", 0.0),
                number("source_height", 0.0),
                boolean("distance_field", false),
                number("distance_field_spread", 8.0),
                number("thickness", 0.0),
                number("softness", 0.0),
                number("outline_width", 0.0),
                color("outline_color", Color::rgba8(0, 0, 0, 0)),
            ]);
        }
        Element::Sdf => {
            properties.extend([
                color("fill_color", Color::rgba8(255, 255, 255, 255)),
                color("stroke_color", Color::rgba8(0, 0, 0, 0)),
                number("stroke_width", 0.0),
                // Extra edge softness in logical pixels, on top of the
                // derivative-based antialiasing the shader always applies. A
                // field is resolution independent, so this is the one knob that
                // turns a crisp edge into a glow.
                number("softness", 0.0),
                // The seam radius every absorbed layer uses unless it names its
                // own. A field with a blend fuses what it contains; a field
                // without one composes the same shapes with hard edges.
                number("blend", 0.0),
                // One position along the morph for the whole composition. A
                // compound shape — a disc with a ring and a notch, say — is
                // several layers that have to move together, and keeping that
                // many numbers in step by hand is how a configuration acquires
                // a frame runtime. Driving them from here makes the compound
                // one animatable property.
                number("morph_progress", 0.0),
                // Everything below belonged to a rectangle alone, because a
                // rectangle had its own pipeline and a composed shape did not.
                // One pipeline draws both now, so a star can carry a gradient
                // and a shadow like anything else.
                any("gradient", Value::Map(BTreeMap::new())),
                color("shadow_color", Color::rgba8(0, 0, 0, 0)),
                number("shadow_blur", 0.0),
                number("shadow_spread", 0.0),
                number("shadow_offset_x", 0.0),
                number("shadow_offset_y", 0.0),
                boolean("shadow_inner", false),
                // Where the stroke sits against the edge: inside, centred or
                // outside. A rectangle border has always been inside and a
                // field stroke centred; they are one outline now, so both are
                // sayable on either.
                string("stroke_alignment", "centre"),
            ]);
        }
        Element::SdfShape => {
            properties.extend([
                string("shape", "circle"),
                // A letter, as a shape in the composition rather than as text
                // drawn beside it. Naming one makes this layer that letter's
                // outline, which then unions, subtracts and morphs with a
                // circle by the same arithmetic a circle does — so a numeral
                // cut out of a disc is a subtraction, and the disc becoming a
                // square while the numeral becomes another is one animation.
                //
                // `glyph_morph_to` names the letter it turns into, walked at
                // `morph_progress` alongside whatever the shapes are doing.
                string("glyph", ""),
                string("glyph_morph_to", ""),
                // A drawing, on exactly the same terms. An SVG is a set of
                // closed curves and so is a letter, so naming a file here makes
                // this layer that drawing's outline — which then unions,
                // subtracts and morphs like every other shape, including into a
                // letter or a circle. Nothing is rasterised on the way: a
                // picture of a shape has pixels rather than points, and there is
                // nothing in a picture to walk onto anything else.
                //
                // `source_morph_to` names the drawing it turns into, walked at
                // `morph_progress` beside whatever the shapes are doing.
                string("source", ""),
                string("source_morph_to", ""),
                // Which face the letter is cut from, and which the letter it
                // turns into is cut from. Empty means the same face, which is
                // the ordinary case; naming a second one morphs across faces,
                // since matching two outlines is geometry and does not care
                // which font either of them came out of.
                string("font_family", "sans-serif"),
                string("font_family_morph_to", ""),
                // The layer's own fill. Fully transparent means "take the
                // field's", which is what keeps a single-colour composition
                // from having to repeat itself on every layer.
                color("fill_color", Color::rgba8(0, 0, 0, 0)),
                // The field this layer becomes at `morph_progress` of one.
                // Interpolating two distance fields passes through shapes that
                // neither end describes, and survives a change of topology —
                // one blob splitting into two — which interpolating outlines
                // cannot do at all.
                string("morph_to", ""),
                // Negative means "follow the field's", so a layer joins the
                // compound morph by saying nothing and leaves it by naming its
                // own position.
                number("morph_progress", -1.0),
                string("operation", "union"),
                // How far either side of the seam a smooth operation blends.
                // Zero is the hard boolean; animating it is what makes two
                // shapes merge and part like liquid.
                number("blend", 0.0),
                number("radius", 0.0),
                // Per-corner overrides, as a Rect carries them: negative means
                // "use the uniform radius". A field box keeps all four, so a
                // rect absorbed into a composition keeps its own shape.
                number("top_left_radius", -1.0),
                number("top_right_radius", -1.0),
                number("bottom_right_radius", -1.0),
                number("bottom_left_radius", -1.0),
                number("points", 5.0),
                number("inner_radius", 0.5),
                number("thickness", 0.0),
                number("angle", 90.0),
            ]);
        }
        Element::Path => {
            properties.extend([
                // SVG path data: `M 0 0 L 10 10 A 5 5 0 0 1 20 20 Z`, every
                // command, absolute or relative.
                string("d", ""),
                // The outline this one turns into, and how far along it is.
                // The two are walked point by point when they have the same
                // run of segments (lines and curves count alike); otherwise
                // the outline changes over at the halfway mark.
                string("morph_to", ""),
                number("morph_progress", 0.0),
                // What SVG does with a path that says nothing: filled black,
                // not stroked, a unit-wide stroke once it has a colour.
                color("fill_color", Color::rgba8(0, 0, 0, 255)),
                string("fill_rule", "nonzero"),
                color("stroke_color", Color::rgba8(0, 0, 0, 0)),
                number("stroke_width", 1.0),
                string("stroke_cap", "butt"),
                string("stroke_join", "miter"),
                number("miter_limit", 4.0),
                // Dash and gap lengths, in path units, repeated; an odd list
                // is read twice over, as SVG reads it.
                any("dash", Value::List(Vec::new())),
                number("dash_offset", 0.0),
                // The part of the outline that is stroked, as fractions of
                // its length: a progress ring is `trim_end`, a line drawing
                // itself on is `trim_end` going from zero to one.
                number("trim_start", 0.0),
                number("trim_end", 1.0),
                // `{ x, y, w, h }` (or `{ x, y, width, height }`, or four
                // numbers): the part of path space that fills the node. Empty
                // means path units are the node's own pixels.
                any("view_box", Value::Map(BTreeMap::new())),
                // How a view box that is not the node's shape fits it:
                // `stretch`, `preserve_aspect_fit` or `preserve_aspect_crop`,
                // the words an Image uses.
                string("fill_mode", "stretch"),
            ]);
        }
        Element::Row | Element::Column => {
            properties.extend([
                // One vocabulary for every packing container: `gap` between
                // children, `align` across the packed axis, `justify` along it.
                number("gap", 0.0),
                string("align", "start"),
                string("justify", "start"),
            ]);
        }
        Element::Grid => {
            properties.extend([
                number("columns", 1.0),
                number("gap", 0.0),
                number("row_gap", 0.0),
                number("column_gap", 0.0),
                // Track lists turn a fixed-column grid into a CSS one:
                // `{ "1fr", "auto", 40, { min = 40, max = "1fr" },
                // "repeat(2, 1fr)" }`. Children then place themselves with
                // `layout.column`, `layout.row` and the spans.
                any("template_columns", Value::List(Vec::new())),
                any("template_rows", Value::List(Vec::new())),
                string("align", "stretch"),
                string("justify", "start"),
            ]);
        }
        Element::Custom => {}
        Element::Terminal => {
            properties.extend([
                string("font_family", "monospace"),
                number("font_size", 13.0),
                // `{ foreground, background, cursor, cursor_text, palette =
                // { sixteen colours } }`; anything left out is the default.
                any("colors", Value::Map(BTreeMap::new())),
                // Between the node's edge and the grid, on every side; the
                // default background fills it.
                number("padding", 0.0),
                // Kept by the runtime, for bindings to follow; read-only to a
                // configuration. The grid's size in cells, what the program
                // last called itself, and whether it is still running.
                number("columns", 0.0),
                number("rows", 0.0),
                string("title", ""),
                boolean("running", false),
                any("exit_code", Value::Nil),
                // The pointer over a terminal is a text beam, as over any text.
                string("cursor", "text"),
            ]);
        }
        Element::Flex => {
            properties.extend([
                string("direction", "row"),
                boolean("wrap", false),
                number("gap", 0.0),
                number("padding", 0.0),
                // `align` is across the direction, `justify` along it, and
                // `align_content` is how wrapped lines share the cross axis.
                // A layout stretches by default, as CSS does; a positioner
                // (`Row`, `Column`) leaves children their own size.
                string("align", "stretch"),
                string("justify", "start"),
                string("align_content", "stretch"),
            ]);
        }
    }
    properties
}

pub(crate) fn any(name: &'static str, default: Value) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyType::Any,
        default,
    }
}

pub(crate) fn boolean(name: &'static str, default: bool) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyType::Bool,
        default: Value::Bool(default),
    }
}

pub(crate) fn number(name: &'static str, default: f64) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyType::Number,
        default: Value::Number(default),
    }
}

pub(crate) fn string(name: &'static str, default: &str) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyType::String,
        default: Value::String(default.to_owned()),
    }
}

pub(crate) fn color(name: &'static str, default: Color) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyType::Color,
        default: Value::Color(default),
    }
}
