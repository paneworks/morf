//! A text input, as paint sees it: text, and what editing adds to it.

use morf_layout::{Geometry, Transform2D};
use morf_scene::{Color, NodeHandle, Scene};

use crate::{RenderError, commands::*, effects::*, paint::*};

/// A text input: its text, clipped to its box, with what editing adds.
///
/// Drawn as text rather than as a kind of its own, so the field is set by the
/// same shaper and the same glyph pipeline as every label beside it; the caret
/// and the selection ride along on the command.
#[allow(clippy::too_many_arguments)]
pub(crate) fn text_input_command(
    scene: &Scene,
    node: NodeHandle,
    bounds: Geometry,
    transform: Transform2D,
    clip: Option<Geometry>,
    inherited: &PaintContext,
    color_overlay: Color,
) -> Result<DrawCommand, RenderError> {
    let scene_error = |error: morf_layout::LayoutError| RenderError::Scene(error.to_string());
    let shape =
        morf_layout::InputShape::read(scene, node, Some(bounds.width)).map_err(scene_error)?;
    let text = scene.string_value(node, "text")?;
    let text_color = resolved_color(scene, node, inherited)?;
    // The content scrolls under the box, so the box is the clip — whatever
    // else is clipping above it.
    let own = transform.bounds(bounds);
    let clip = Some(clip.map_or(own, |clip| intersect_geometry(clip, own)));
    let focused = scene.bool_value(node, "focus")? && scene.bool_value(node, "enabled")?;
    let offset = |property: &str| -> Result<usize, RenderError> {
        let byte = scene.number(node, property)?.max(0.0) as usize;
        Ok(shape.display.to_display(text, byte.min(text.len())))
    };
    let cursor = offset("cursor_position")?;
    let (start, end) = (offset("selection_start")?, offset("selection_end")?);
    let caret = (focused
        && !scene.bool_value(node, "read_only")?
        && scene.bool_value(node, "caret_visible")?)
    .then_some(cursor);
    let caret_color = match scene.color_value(node, "caret_color")? {
        color if color.alpha > 0.0 => color,
        _ => text_color,
    };
    let edit = TextEdit {
        scroll: (
            scene.number(node, "scroll_x")?,
            scene.number(node, "scroll_y")?,
        ),
        selection: if focused {
            start.min(end)..start.max(end)
        } else {
            0..0
        },
        selection_color: scene.color_value(node, "selection_color")?,
        selected_text_color: scene.color_value(node, "selected_text_color")?,
        caret,
        caret_color,
        caret_width: scene.number(node, "caret_width")?.max(0.0),
        placeholder: shape.display.placeholder,
    };
    // A code editor's colours, over what is typed (never over a placeholder
    // or a password's dots).
    let mut style = shape.options.style;
    if !edit.placeholder
        && !scene.bool_value(node, "password")?
        && let Some(rich) = morf_scene::RichText::from_highlights(
            &shape.display.text,
            scene.current(node, "highlights")?,
        )
        .map_err(|message| RenderError::Scene(format!("TextInput highlights: {message}")))?
    {
        style.rich = Some(std::sync::Arc::new(rich));
    }
    Ok(DrawCommand::Text {
        node,
        bounds,
        transform,
        clip,
        text: shape.display.text,
        family: shape.family,
        font_source: shape.options.font_source.unwrap_or_default(),
        size: shape.size,
        font_weight: shape.options.font_weight,
        color: if edit.placeholder {
            scene.color_value(node, "placeholder_color")?
        } else {
            text_color
        },
        color_overlay,
        wrap: shape.options.wrap,
        max_lines: 0,
        elide: shape.options.elide,
        horizontal_alignment: shape.options.alignment,
        // Several lines start at the top and scroll; one sits where it is put.
        vertical_alignment: if shape.multiline {
            crate::VerticalAlignment::Top
        } else {
            vertical_alignment(scene.string_value(node, "vertical_alignment")?)?
        },
        field_style: DistanceFieldStyle::default(),
        morph_to: String::new(),
        morph_progress: 0.0,
        style,
        decoration: None,
        edit: Some(Box::new(edit)),
    })
}
