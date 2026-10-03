//! What a property accepts, and the value it keeps.
//!
//! Every write goes through here: a type is checked, a colour string
//! becomes a colour, and the few properties with a shape of their own — a
//! gradient, a decoration, a line height, a cursor — are refused where they
//! are written rather than where they are painted.

use crate::{animation::*, decoration::TextDecoration, gradient::Gradient, types::*};

/// Every shape a `MouseArea` may ask the pointer to take: the cursor-shape
/// protocol's own list, spelled with underscores.
pub const CURSOR_SHAPES: [&str; 36] = [
    "default",
    "context_menu",
    "help",
    "pointer",
    "progress",
    "wait",
    "cell",
    "crosshair",
    "text",
    "vertical_text",
    "alias",
    "copy",
    "move",
    "no_drop",
    "not_allowed",
    "grab",
    "grabbing",
    "e_resize",
    "n_resize",
    "ne_resize",
    "nw_resize",
    "s_resize",
    "se_resize",
    "sw_resize",
    "w_resize",
    "ew_resize",
    "ns_resize",
    "nesw_resize",
    "nwse_resize",
    "col_resize",
    "row_resize",
    "all_scroll",
    "zoom_in",
    "zoom_out",
    "dnd_ask",
    "all_resize",
];

/// Every key `anchors` understands.
pub const ANCHOR_KEYS: &[&str] = &[
    "fill",
    "center_in",
    "horizontal_center",
    "vertical_center",
    "left",
    "right",
    "top",
    "bottom",
    "margins",
    "left_margin",
    "right_margin",
    "top_margin",
    "bottom_margin",
    "horizontal_center_offset",
    "vertical_center_offset",
];

pub(crate) fn coerce(
    element: Element,
    property: &str,
    kind: PropertyType,
    value: Value,
) -> Result<Value, SceneError> {
    let invalid = |message: String| SceneError::InvalidPropertyValue {
        element: element.name(),
        property: property.to_owned(),
        message,
    };
    match property {
        "gradient" => return Gradient::canonical(value).map_err(invalid),
        "mask" => return crate::mask::MaskSpec::canonical(value).map_err(invalid),
        "decoration" => return TextDecoration::canonical(value).map_err(invalid),
        "spans" if element == Element::Text => {
            return crate::rich_text::canonical_spans(value).map_err(invalid);
        }
        "line_height" => {
            // A bare number is a multiple of the font size; a `px` string is
            // a size. Checked here so a wrong one is refused where written.
            return match &value {
                Value::Number(multiple) if multiple.is_finite() && *multiple > 0.0 => Ok(value),
                Value::String(text)
                    if text
                        .strip_suffix("px")
                        .and_then(|pixels| pixels.trim().parse::<f64>().ok())
                        .is_some_and(|pixels| pixels.is_finite() && pixels > 0.0) =>
                {
                    Ok(value)
                }
                _ => Err(invalid(
                    "a multiple of the font size or a `px` size".to_owned(),
                )),
            };
        }
        "optical_sizing" if matches!(element, Element::Text | Element::TextInput) => {
            // CSS's words, or a boolean for whether it is on.
            return match &value {
                Value::Bool(true) => Ok(Value::String("auto".to_owned())),
                Value::Bool(false) => Ok(Value::String("none".to_owned())),
                Value::String(name) if matches!(name.as_str(), "auto" | "none") => Ok(value),
                _ => Err(invalid(
                    "optical_sizing is \"auto\", \"none\", true or false".to_owned(),
                )),
            };
        }
        "anchors" => {
            // An anchor name nothing reads used to be dropped without a word,
            // and a node with a misspelt `center_in` simply sat in the corner.
            if let Value::Map(map) = &value
                && let Some(unknown) = map.keys().find(|key| !ANCHOR_KEYS.contains(&key.as_str()))
            {
                return Err(invalid(format!(
                    "`{unknown}` is not an anchor: use {}",
                    ANCHOR_KEYS.join(", ")
                )));
            }
        }
        // A matrix is numbers in a fixed count, checked here so a wrong one is
        // refused where it is written rather than drawn as nothing.
        "transform_matrix" | "matrix"
            if property == "transform_matrix" || element == Element::SdfShape =>
        {
            let lengths: &[usize] = if property == "matrix" { &[4] } else { &[4, 6] };
            match &value {
                Value::Nil => {}
                Value::List(items)
                    if lengths.contains(&items.len())
                        && items
                            .iter()
                            .all(|item| matches!(item, Value::Number(n) if n.is_finite())) => {}
                _ => {
                    return Err(invalid(if property == "matrix" {
                        "a list of four numbers { a, b, c, d }".to_owned()
                    } else {
                        "a list of six numbers { a, b, c, d, tx, ty } (or four)".to_owned()
                    }));
                }
            }
        }
        "blend_profile" if element == Element::Sdf => {
            if let Value::String(name) = &value
                && !matches!(name.as_str(), "quadratic" | "circular")
            {
                return Err(invalid(format!("`{name}` is not quadratic or circular")));
            }
        }
        "focus_policy" => {
            if let Value::String(name) = &value
                && crate::FocusPolicy::parse(name).is_none()
            {
                return Err(invalid(format!(
                    "`{name}` is not auto, none, click, tab or strong"
                )));
            }
        }
        "cursor" => {
            if let Value::String(name) = &value
                && !CURSOR_SHAPES.contains(&name.as_str())
            {
                return Err(invalid(format!("`{name}` is not a cursor shape")));
            }
        }
        // A path's words are checked where they are written, so a misspelt
        // cap is an error at the node rather than a stroke that ends wrong.
        "stroke_cap" | "stroke_join" | "fill_rule" if element == Element::Path => {
            if let Value::String(name) = &value {
                let known = match property {
                    "stroke_cap" => crate::StrokeCap::parse(name).is_some(),
                    "stroke_join" => crate::StrokeJoin::parse(name).is_some(),
                    _ => crate::FillRule::parse(name).is_some(),
                };
                if !known {
                    return Err(invalid(format!(
                        "`{name}` is not one of {}",
                        match property {
                            "stroke_cap" => "butt, round, square",
                            "stroke_join" => "miter, round, bevel",
                            _ => "nonzero, evenodd",
                        }
                    )));
                }
            }
        }
        "d" | "morph_to" if element == Element::Path => {
            if let Value::String(data) = &value
                && !data.trim().is_empty()
            {
                kurbo::BezPath::from_svg(data)
                    .map_err(|error| invalid(format!("not SVG path data: {error}")))?;
            }
        }
        "view_box" if element == Element::Path => {
            crate::PathViewBox::parse(&value).map_err(invalid)?;
        }
        "dash" if element == Element::Path => {
            crate::path_dash(&value).map_err(invalid)?;
        }
        "fill_mode" if element == Element::Path => {
            if let Value::String(name) = &value
                && !matches!(
                    name.as_str(),
                    "stretch" | "preserve_aspect_fit" | "preserve_aspect_crop"
                )
            {
                return Err(invalid(format!(
                    "`{name}` is not stretch, preserve_aspect_fit or preserve_aspect_crop"
                )));
            }
        }
        "font_style" => {
            if let Value::String(name) = &value
                && !matches!(name.as_str(), "normal" | "italic" | "oblique")
            {
                return Err(invalid(format!(
                    "`{name}` is not normal, italic or oblique"
                )));
            }
        }
        "axes" if matches!(element, Element::Text | Element::TextInput) => {
            // An empty table is an empty map, so it moves to and from one.
            if matches!(&value, Value::List(list) if list.is_empty()) || value == Value::Nil {
                return Ok(Value::Map(Default::default()));
            }
            let Value::Map(axes) = &value else {
                return Err(invalid(
                    "axes is a table of four-letter tags to numbers".to_owned(),
                ));
            };
            for (tag, number) in axes {
                if tag.len() != 4 || !tag.bytes().all(|byte| (0x20..0x7f).contains(&byte)) {
                    return Err(invalid(format!("`{tag}` is not a four-letter axis tag")));
                }
                if !matches!(number, Value::Number(number) if number.is_finite()) {
                    return Err(invalid(format!("axis `{tag}` must be a number")));
                }
            }
        }
        "font_stretch" => {
            if let Value::String(name) = &value
                && !matches!(
                    name.as_str(),
                    "ultra_condensed"
                        | "extra_condensed"
                        | "condensed"
                        | "semi_condensed"
                        | "normal"
                        | "semi_expanded"
                        | "expanded"
                        | "extra_expanded"
                        | "ultra_expanded"
                )
            {
                return Err(invalid(format!(
                    "`{name}` is not a width from ultra_condensed to ultra_expanded"
                )));
            }
        }
        _ => {}
    }
    // `color` is the one property that may say "inherit": text takes the
    // nearest ancestor's colour, and an `Item` carries one for it without
    // painting anything.
    let converted = match (kind, value) {
        (_, Value::String(value)) if property == "color" && value == "inherit" => {
            Some(Value::String(value))
        }
        (PropertyType::Any, Value::String(value)) if property == "color" => {
            Color::parse(&value).map(Value::Color)
        }
        (PropertyType::Any, value @ (Value::Nil | Value::Color(_))) if property == "color" => {
            Some(value)
        }
        (PropertyType::Any, _) if property == "color" => None,
        (PropertyType::Any, value) => Some(value),
        (PropertyType::Bool, Value::Bool(value)) => Some(Value::Bool(value)),
        (PropertyType::Number, Value::Number(value)) if value.is_finite() => {
            Some(Value::Number(value))
        }
        (PropertyType::String, Value::String(value)) => Some(Value::String(value)),
        (PropertyType::Color, Value::Color(value)) => Some(Value::Color(value)),
        (PropertyType::Color, Value::String(value)) => Color::parse(&value).map(Value::Color),
        _ => None,
    };
    converted.ok_or_else(|| SceneError::InvalidPropertyType {
        element: element.name(),
        property: property.to_owned(),
        expected: match kind {
            PropertyType::Any if property == "color" => "color, inherit or nil",
            PropertyType::Any => "value",
            PropertyType::Bool => "boolean",
            PropertyType::Number => "finite number",
            PropertyType::String => "string",
            PropertyType::Color => "color",
        },
    })
}
