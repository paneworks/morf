//! The draw command each kind of node paints for itself: text, images,
//! icons and field compositions, read off the scene.

use super::*;

/// A `Text` node's shaped run, as it stands.
pub(super) fn text_command(
    scene: &Scene,
    node: NodeHandle,
    bounds: Geometry,
    transform: Transform2D,
    clip: Option<Geometry>,
    inherited: &PaintContext,
    color_overlay: Color,
) -> Result<DrawCommand, RenderError> {
    Ok(DrawCommand::Text {
        node,
        bounds,
        transform,
        clip,
        text: scene.string_value(node, "text")?.to_owned(),
        family: scene.string_value(node, "font_family")?.to_owned(),
        font_source: scene.string_value(node, "font_source")?.to_owned(),
        size: scene.number(node, "font_size")?,
        font_weight: scene.number(node, "font_weight")?,
        color: resolved_color(scene, node, inherited)?,
        color_overlay,
        wrap: scene.bool_value(node, "wrap")?,
        max_lines: scene.number(node, "max_lines")?.max(0.0) as usize,
        elide: render_text_elide(scene.string_value(node, "elide")?)?,
        horizontal_alignment: render_text_alignment(scene.directed_alignment(node)?)?,
        vertical_alignment: vertical_alignment(scene.string_value(node, "vertical_alignment")?)?,
        field_style: text_field_style(scene, node)?,
        morph_to: scene.string_value(node, "morph_to")?.to_owned(),
        morph_progress: scene.number(node, "morph_progress")?.clamp(0.0, 1.0) as f32,
        style: morf_layout::TextStyle::from_scene(scene, node)
            .map_err(|error| RenderError::Scene(error.to_string()))?,
        decoration: morf_scene::TextDecoration::parse(scene.current(node, "decoration")?)
            .map_err(RenderError::Scene)?,
        edit: None,
    })
}

/// An `Image` node's picture.
pub(super) fn image_command(
    scene: &Scene,
    node: NodeHandle,
    bounds: Geometry,
    transform: Transform2D,
    clip: Option<Geometry>,
    color_overlay: Color,
) -> Result<DrawCommand, RenderError> {
    Ok(DrawCommand::Texture {
        node,
        bounds,
        transform,
        clip,
        source: scene.string_value(node, "source")?.to_owned(),
        icon_theme: None,
        color_overlay,
        fill_mode: image_fill_mode(scene.string_value(node, "fill_mode")?)?,
        smooth: scene.bool_value(node, "smooth")?,
        distance_field: scene.bool_value(node, "distance_field")?,
        distance_field_spread: scene.number(node, "distance_field_spread")?.max(0.5) as f32,
        distance_field_style: text_field_style(scene, node)?,
        frame: scene.number(node, "frame")?.max(0.0) as u32,
    })
}

/// An `Icon` node's themed picture.
pub(super) fn icon_command(
    scene: &Scene,
    node: NodeHandle,
    bounds: Geometry,
    transform: Transform2D,
    clip: Option<Geometry>,
    color_overlay: Color,
) -> Result<DrawCommand, RenderError> {
    Ok(DrawCommand::Texture {
        node,
        bounds,
        transform,
        clip,
        source: scene.string_value(node, "name")?.to_owned(),
        icon_theme: Some(scene.string_value(node, "theme")?.to_owned()),
        color_overlay,
        fill_mode: image_fill_mode(scene.string_value(node, "fill_mode")?)?,
        smooth: true,
        distance_field: scene.bool_value(node, "distance_field")?,
        distance_field_spread: scene.number(node, "distance_field_spread")?.max(0.5) as f32,
        distance_field_style: text_field_style(scene, node)?,
        frame: 0,
    })
}

/// A `Sdf` node's composition, or nothing when no shape is in it.
pub(super) fn field_command(
    scene: &Scene,
    layout: &Layout,
    node: NodeHandle,
    bounds: Geometry,
    transform: Transform2D,
    clip: Option<Geometry>,
    color_overlay: Color,
) -> Result<Option<DrawCommand>, RenderError> {
    let mut layers = Vec::new();
    let defaults = FieldDefaults {
        blend: scene.number(node, "blend")?.max(0.0) as f32,
        color: apply_overlay(scene.color_value(node, "fill_color")?, color_overlay),
        morph: scene.number(node, "morph_progress")?.clamp(0.0, 1.0) as f32,
        overlay: color_overlay,
        profile: BlendProfile::parse(scene.string_value(node, "blend_profile")?)
            .unwrap_or_default(),
        transform,
        opacity: 1.0,
    };
    field_layers(scene, layout, node, defaults, &mut layers)?;
    // A composition with nothing in it has no zero crossing and would
    // paint the whole rectangle, so it draws nothing at all.
    if layers.is_empty() {
        return Ok(None);
    }
    Ok(Some(DrawCommand::Field {
        node,
        bounds,
        transform,
        clip,
        fill_color: apply_overlay(scene.color_value(node, "fill_color")?, color_overlay),
        stroke_color: apply_overlay(scene.color_value(node, "stroke_color")?, color_overlay),
        stroke_width: scene.number(node, "stroke_width")?.max(0.0),
        stroke_alignment: stroke_alignment(scene.string_value(node, "stroke_alignment")?)?,
        softness: scene.number(node, "softness")?.max(0.0),
        gradient: scene_gradient(scene, node)?,
        color_overlay,
        shadow_color: scene.color_value(node, "shadow_color")?,
        shadow_blur: scene.number(node, "shadow_blur")?.max(0.0),
        shadow_spread: scene.number(node, "shadow_spread")?,
        shadow_offset_x: scene.number(node, "shadow_offset_x")?,
        shadow_offset_y: scene.number(node, "shadow_offset_y")?,
        shadow_inner: scene.bool_value(node, "shadow_inner")?,
        // An effect shader belongs to the layer that composites
        // this node, not to the node's own fill: leaving it here
        // too would have the field pass look for a program that
        // was registered against the composite pass.
        shader: shader_binding(scene, node)?.filter(|shader| !shader.samples_behind),
        layers,
    }))
}
