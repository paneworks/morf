use std::collections::BTreeMap;

use crate::{animation::*, types::*};

pub(crate) use crate::coerce::coerce;

mod shapes;
mod text;

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
        // An alpha mask over the node and its subtree: `{ gradient = ... }`
        // here; a node given as the mask lives in `Scene::set_mask` instead.
        any("mask", Value::Map(BTreeMap::new())),
        // Whether the mask keeps what it covers (false) or cuts it out.
        boolean("mask_invert", false),
        color("color_overlay", Color::rgba8(0, 0, 0, 0)),
        number("z", 0.0),
        boolean("clip", element == Element::ClipRect),
        // `true` asks the compositor to blur what is behind this node.
        //
        // On every element rather than only the drawn ones, because what it
        // marks is an area of the surface, not a way of painting: an `Item`
        // wrapping a panel is often the honest place to say it.
        //
        // A number is a radius instead, and the engine does the blurring: on
        // a `Rect` or `ClipRect` it frosts whatever this same surface drew
        // beneath the shape, which works on any compositor, blur or none.
        any("backdrop_blur", Value::Bool(false)),
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
        // An affine map `{ a, b, c, d, tx, ty }` (or just `{ a, b, c, d }`)
        // applied about the transform origin, inside `scale`, `rotation` and
        // `skew`: the node is drawn through it, its children with it, and a
        // pointer is mapped back through it.
        any("transform_matrix", Value::Nil),
        boolean("enabled", true),
        boolean("focus", false),
        // Whether Tab moves the keyboard away from this node while it has
        // it. False hands Tab to its own `on_key_pressed`. A terminal keeps
        // it: Tab is how a shell completes.
        boolean("tab_navigation", element != Element::Terminal),
        // How the node takes focus (`focus.rs`): "auto" -- by click and by
        // Tab when it takes keys --, "none", "click", "tab" or "strong".
        string("focus_policy", "auto"),
        // A group that remembers which of its nodes last had focus: Tab
        // into it lands there, and focus lost inside it stays inside it.
        boolean("focus_scope", false),
        // Whether the node has focus now, and whether a keyboard gave it --
        // the ring a theme draws. The runtime writes both.
        boolean("focused", false),
        boolean("visual_focus", false),
        any("layout", Value::Map(BTreeMap::new())),
        // A name for the node that nothing in the engine reads: it is for
        // whoever has to find the node again from outside -- `morf test`'s
        // `test.find { id = ... }`, a log line.
        string("id", ""),
        // "ltr" or "rtl" for this subtree; "" takes the parent's
        // (`direction.rs`).
        string("layout_direction", ""),
        // What a screen reader is told (`accessible.rs`): the role, name and
        // description, and one table of the rest -- `value`, `minimum`,
        // `maximum`, `step`, `checked`, `expanded`, `selected`, `disabled`,
        // `pressed`, `read_only`, `modal`, `level`, `orientation`,
        // `placeholder` -- so a node that says nothing pays for one slot.
        // `accessible_hidden` takes the subtree out.
        string("accessible_role", ""),
        string("accessible_name", ""),
        string("accessible_description", ""),
        any("accessible", Value::Nil),
        boolean("accessible_hidden", false),
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
            // Hide the item when deactivated rather than destroy it.
            boolean("keep", false),
            // Build the item ahead of time, hidden, while nothing moves.
            boolean("preload", false),
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
                // How saturated a frosted backdrop is: 1 leaves it as it was.
                number("backdrop_saturation", 1.0),
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
        Element::Text => properties.extend(text::text()),
        Element::TextInput => properties.extend(text::text_input()),
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
        Element::Sdf => properties.extend(shapes::sdf()),
        Element::SdfShape => properties.extend(shapes::sdf_shape()),
        Element::Path => properties.extend(shapes::path()),
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

/// Every element, as the configuration names it (`ui.Rect`), in declaration
/// order.
pub const ELEMENTS: &[Element] = &[
    Element::Item,
    Element::Inset,
    Element::Rect,
    Element::ClipRect,
    Element::Text,
    Element::TextInput,
    Element::Image,
    Element::Icon,
    Element::Sdf,
    Element::SdfShape,
    Element::Path,
    Element::MouseArea,
    Element::DropArea,
    Element::Row,
    Element::Column,
    Element::Grid,
    Element::Flickable,
    Element::Loader,
    Element::Timer,
    Element::Flex,
    Element::Custom,
    Element::Terminal,
];

/// One property as the outside sees it: its name, its kind (`number`,
/// `boolean`, `string`, `color`, `any`) and its default.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyInfo {
    pub name: &'static str,
    pub kind: &'static str,
    pub default: Value,
}

/// Every element's name and properties: for tools that describe the API
/// (`morf types`), so what they say is what the engine takes.
pub fn element_schemas() -> Vec<(&'static str, Vec<PropertyInfo>)> {
    ELEMENTS
        .iter()
        .map(|&element| {
            let properties = schema(element)
                .into_iter()
                .map(|spec| PropertyInfo {
                    name: spec.name,
                    kind: match spec.kind {
                        PropertyType::Any => "any",
                        PropertyType::Bool => "boolean",
                        PropertyType::Number => "number",
                        PropertyType::String => "string",
                        PropertyType::Color => "color",
                    },
                    default: spec.default,
                })
                .collect();
            (element.name(), properties)
        })
        .collect()
}
