//! Node properties read for painting: a node's mask, a rect's shadow, the
//! plain grouping layer, and how text thresholds its glyph fields.

use super::*;

/// What masks a node: the subtree it was given as its mask, while that is
/// visible, or else the gradient its `mask` property holds.
#[derive(Clone)]
pub(super) enum MaskSource {
    Gradient(morf_scene::Gradient),
    Node(NodeHandle),
}

pub(super) fn mask_source(
    scene: &Scene,
    node: NodeHandle,
) -> Result<Option<MaskSource>, RenderError> {
    if let Some(mask) = scene.mask(node)
        && scene.bool_value(mask, "visible")?
    {
        return Ok(Some(MaskSource::Node(mask)));
    }
    let value = scene.current(node, "mask")?;
    // Nearly every node has none, and an empty table has nothing to parse.
    if matches!(value, morf_scene::Value::Map(entries) if entries.is_empty()) {
        return Ok(None);
    }
    Ok(morf_scene::MaskSpec::parse(value)
        .map_err(RenderError::Scene)?
        .map(|spec| MaskSource::Gradient(spec.gradient)))
}

/// A gradient mask, as the white quad it is drawn with: the gradient is
/// the alpha the masked layer is composited through.
pub(super) fn gradient_mask(
    node: NodeHandle,
    bounds: Geometry,
    transform: Transform2D,
    clip: Option<Geometry>,
    gradient: morf_scene::Gradient,
) -> DrawCommand {
    DrawCommand::Quad {
        node,
        bounds,
        transform,
        clip,
        color: Color::rgba8(255, 255, 255, 255),
        color_overlay: Color::rgba8(0, 0, 0, 0),
        gradient: Some(gradient),
        radii: [0.0; 4],
        border_width: 0.0,
        antialiasing: true,
        border_pixel_aligned: false,
        border_color: Color::rgba8(0, 0, 0, 0),
        blur: 0.0,
        shadow_color: Color::rgba8(0, 0, 0, 0),
        shadow_blur: 0.0,
        shadow_spread: 0.0,
        shadow_offset_x: 0.0,
        shadow_offset_y: 0.0,
        shadow_inner: false,
        shader: None,
    }
}

/// A layer that only groups: composited as it is, at full opacity.
pub(super) fn plain_layer(
    node: NodeHandle,
    start: usize,
    parent: Option<usize>,
    bounds: Geometry,
) -> Layer {
    Layer {
        node,
        commands: start..start,
        parent,
        opacity: 1.0,
        blur: 0.0,
        shadow_color: Color::rgba8(0, 0, 0, 0),
        shadow_blur: 0.0,
        shadow_offset: [0.0, 0.0],
        mask: None,
        shader: None,
        bounds,
        alpha_mask: None,
        mask_for: None,
    }
}

/// How a text node wants its glyph fields thresholded.
///
/// Thickness is in logical pixels of edge movement, which is what a
/// configuration can reason about: asking for half a pixel more weight means
/// the same thing at every size, where a shift in field units would not.
pub(super) fn text_field_style(
    scene: &Scene,
    node: NodeHandle,
) -> Result<DistanceFieldStyle, RenderError> {
    Ok(DistanceFieldStyle {
        thickness: scene.number(node, "thickness")? as f32,
        softness: scene.number(node, "softness")?.max(0.0) as f32,
        outline_width: scene.number(node, "outline_width")?.max(0.0) as f32,
        outline_color: scene.color_value(node, "outline_color")?,
    })
}

/// A rect's drop shadow, read only as far as it is visible.
pub(super) struct RectShadow {
    pub(super) color: Color,
    pub(super) blur: f64,
    pub(super) spread: f64,
    pub(super) offset_x: f64,
    pub(super) offset_y: f64,
    pub(super) inner: bool,
}

impl RectShadow {
    pub(super) fn none() -> Self {
        Self {
            color: Color::rgba8(0, 0, 0, 0),
            blur: 0.0,
            spread: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
            inner: false,
        }
    }
}

pub(super) fn rect_shadow(scene: &Scene, node: NodeHandle) -> Result<RectShadow, RenderError> {
    let color = scene.color_value(node, "shadow_color")?;
    if color.alpha <= 0.0 {
        return Ok(RectShadow::none());
    }
    Ok(RectShadow {
        color,
        blur: scene.number(node, "shadow_blur")?.max(0.0),
        spread: scene.number(node, "shadow_spread")?,
        offset_x: scene.number(node, "shadow_offset_x")?,
        offset_y: scene.number(node, "shadow_offset_y")?,
        inner: scene.bool_value(node, "shadow_inner")?,
    })
}
