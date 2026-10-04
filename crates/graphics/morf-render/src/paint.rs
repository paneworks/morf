use morf_layout::{Geometry, Layout, Transform2D, node_transform};
use morf_scene::{Color, Element, NodeHandle, Scene};

use crate::{commands::*, effects::*, paint_fields::*, paint_text_input::*, sdf::*};

mod element_commands;
mod path_paint;
mod properties;

use element_commands::*;
use path_paint::path_paint;
use properties::*;

#[derive(Clone, Copy)]
pub(crate) struct PaintContext {
    pub(crate) transform: Transform2D,
    pub(crate) clip: Option<Geometry>,
    pub(crate) overlay: Color,
    pub(crate) layer: Option<usize>,
    /// Whether an enclosing field has already taken this node's shape.
    pub(crate) in_field: bool,
    /// The nearest ancestor's colour, for text that says `inherit`.
    pub(crate) color: Option<Color>,
}

/// The colour a node paints with: its own, or the nearest ancestor's when it
/// says `inherit`, or black when nothing above it has one.
pub(crate) fn resolved_color(
    scene: &Scene,
    node: NodeHandle,
    inherited: &PaintContext,
) -> Result<Color, RenderError> {
    Ok(match scene.current(node, "color")? {
        morf_scene::Value::Color(color) => *color,
        _ => inherited.color.unwrap_or(Color::rgba8(0, 0, 0, 255)),
    })
}

pub(crate) fn append_node(
    scene: &Scene,
    layout: &Layout,
    node: NodeHandle,
    inherited: PaintContext,
    list: &mut DrawList,
) -> Result<(), RenderError> {
    if !scene.bool_value(node, "visible")? {
        return Ok(());
    }
    let node_opacity = scene.number(node, "opacity")?.clamp(0.0, 1.0);
    let rotation = scene.number(node, "rotation")?;
    let layer_config = layer_config(scene, node)?;
    let element = scene.element(node)?;
    let rect_blur = if matches!(element, Element::Rect | Element::ClipRect) {
        scene.number(node, "blur")?.max(0.0)
    } else {
        0.0
    };
    let layer_blur = layer_config.blur.max(rect_blur);
    // Both are asked for more than once further down, and each `rect_radii` is
    // five property reads of its own.
    let clips = scene.bool_value(node, "clip")?;
    let radii = if matches!(element, Element::Rect | Element::ClipRect) {
        rect_radii(scene, node)?
    } else {
        [0.0; 4]
    };
    let rounded_clip = clips
        && matches!(element, Element::Rect | Element::ClipRect)
        && radii.iter().any(|radius| *radius > 0.0);
    // An effect shader composites the subtree rather than colouring one node,
    // so it is taken off the node here and given to the layer below.
    let effect = shader_binding(scene, node)?.filter(|shader| shader.samples_behind);
    // A shape an enclosing field composes is not drawn as a node at all — its
    // rotation is one of the numbers the field is given, and the field turns
    // the sample point by it. Making a layer to rotate it into would be an
    // offscreen target for a subtree that paints nothing.
    //
    // It was also wrong, not merely wasteful. The layer came out empty, an
    // empty layer claims the index of the command that would have followed it,
    // and the frame loop then stepped over that command: a rotated shape inside
    // a field silently ate whatever was drawn next.
    let absorbed = inherited.in_field && absorbed_by_field(element);
    // An alpha mask composites the subtree through another picture, so it
    // needs the subtree in a layer of its own. A shape a field absorbed has
    // no subtree of its own to mask.
    let alpha_mask = if absorbed {
        None
    } else {
        mask_source(scene, node)?
    };
    // A shadow is taken from the layer's alpha and an effect shader owns the
    // composite, so a node with either and a mask gets a second layer around
    // its own for the mask: masked last, the mask cuts the shadow and the
    // effect's output as it cuts everything else.
    let mask_wraps =
        alpha_mask.is_some() && (effect.is_some() || layer_config.shadow_color.alpha > 0.0);
    let outer_mask_layer = mask_wraps.then(|| {
        let index = list.layers.len();
        list.layers.push(plain_layer(
            node,
            list.commands.len(),
            inherited.layer,
            Geometry::default(),
        ));
        index
    });
    let inherited = PaintContext {
        layer: outer_mask_layer.or(inherited.layer),
        ..inherited
    };
    // A shape a field absorbed and that holds nothing else is faded by the
    // field, as a layer of it: an offscreen target for it would be empty.
    let faded_by_field =
        absorbed && element == Element::SdfShape && scene.children(node)?.is_empty();
    let creates_layer = alpha_mask.is_some()
        || layer_config.enabled
        || (node_opacity < 1.0 && !faded_by_field)
        || (rotation != 0.0 && !absorbed)
        || rounded_clip
        || layer_blur > 0.0
        || layer_config.shadow_color.alpha > 0.0
        // An effect shader has nothing to read until its subtree has been
        // rendered somewhere, so a node carrying one becomes a layer whether or
        // not anything else would have made it into one.
        || effect.is_some();
    let layer = creates_layer.then(|| {
        let index = list.layers.len();
        list.layers.push(Layer {
            node,
            shader: effect.clone(),
            commands: list.commands.len()..list.commands.len(),
            parent: inherited.layer,
            opacity: node_opacity as f32,
            blur: layer_blur as f32,
            shadow_color: layer_config.shadow_color,
            shadow_blur: layer_config.shadow_blur as f32,
            shadow_offset: [
                layer_config.shadow_offset_x as f32,
                layer_config.shadow_offset_y as f32,
            ],
            mask: None,
            bounds: Geometry::default(),
            alpha_mask: None,
            mask_for: None,
        });
        index
    });
    let color_overlay =
        compose_overlay(inherited.overlay, scene.color_value(node, "color_overlay")?);
    let Some(bounds) = layout.geometry(node) else {
        return Ok(());
    };
    let transform = inherited.transform.then(
        node_transform(scene, node, bounds)
            .map_err(|error| RenderError::Scene(error.to_string()))?,
    );
    if let Some(layer) = layer
        && rounded_clip
    {
        list.layers[layer].mask = Some(LayerMask {
            bounds,
            transform,
            radii,
        });
    }
    let clip = if clips {
        let bounds = transform.bounds(bounds);
        Some(
            inherited
                .clip
                .map_or(bounds, |inherited| intersect_geometry(inherited, bounds)),
        )
    } else {
        inherited.clip
    };
    // A shape an enclosing field composed is drawn by that field, not again on
    // its own. Everything else — text, images, anything without a field —
    // paints normally over the composition.
    let painted = !absorbed;
    // A shadow is five numbers and a flag that only matter once the colour is
    // visible, and a rect with no shadow is the overwhelming majority. Asking
    // for the colour first turns six property reads into one for all of them.
    let shadow = if painted && matches!(element, Element::Rect | Element::ClipRect) {
        rect_shadow(scene, node)?
    } else {
        RectShadow::none()
    };
    // Frosted glass goes down first, so the rectangle's own fill tints it.
    if painted
        && matches!(element, Element::Rect | Element::ClipRect)
        && let morf_scene::Value::Number(radius) = scene.current(node, "backdrop_blur")?
        && radius.is_finite()
        && *radius > 0.0
    {
        list.commands.push(DrawCommand::Backdrop {
            node,
            bounds,
            transform,
            clip,
            radii,
            radius: radius.min(MAX_BACKDROP_BLUR),
            saturation: scene.number(node, "backdrop_saturation")?.max(0.0),
        });
    }
    match element {
        Element::Rect | Element::ClipRect if painted => list.commands.push(DrawCommand::Quad {
            node,
            bounds,
            transform,
            clip,
            color: resolved_color(scene, node, &inherited)?,
            color_overlay,
            gradient: scene_gradient(scene, node)?,
            radii,
            border_width: if element == Element::ClipRect {
                0.0
            } else {
                scene.number(node, "border_width")?
            },
            antialiasing: element != Element::ClipRect || scene.bool_value(node, "antialiasing")?,
            border_pixel_aligned: element == Element::ClipRect
                && scene.bool_value(node, "border_pixel_aligned")?,
            border_color: scene.color_value(node, "border_color")?,
            blur: if layer_blur > 0.0 { 0.0 } else { rect_blur },
            shadow_color: shadow.color,
            shadow_blur: shadow.blur,
            shadow_spread: shadow.spread,
            shadow_offset_x: shadow.offset_x,
            shadow_offset_y: shadow.offset_y,
            shadow_inner: shadow.inner,
            // A rectangle wears a shader the same way a field does, because it
            // *is* a field of one layer. An effect shader belongs to the layer
            // that composites the node, not to its own fill.
            shader: shader_binding(scene, node)?.filter(|shader| !shader.samples_behind),
        }),
        Element::Text => list.commands.push(text_command(
            scene,
            node,
            bounds,
            transform,
            clip,
            &inherited,
            color_overlay,
        )?),
        Element::TextInput => {
            list.commands.push(text_input_command(
                scene,
                node,
                bounds,
                transform,
                clip,
                &inherited,
                color_overlay,
            )?);
        }
        Element::Terminal => {
            // Nothing until the runtime has a picture of its screen: a
            // terminal is laid out before its program has written anything.
            if let Some(screen) = scene.terminal_screen(node) {
                list.commands.push(DrawCommand::Terminal {
                    node,
                    bounds,
                    transform,
                    clip,
                    color_overlay,
                    screen: std::sync::Arc::clone(screen),
                });
            }
        }
        Element::Image => list.commands.push(image_command(
            scene,
            node,
            bounds,
            transform,
            clip,
            color_overlay,
        )?),
        Element::Icon => list.commands.push(icon_command(
            scene,
            node,
            bounds,
            transform,
            clip,
            color_overlay,
        )?),
        Element::Path => list.commands.push(DrawCommand::Path {
            node,
            bounds,
            transform,
            clip,
            color_overlay,
            paint: Box::new(path_paint(scene, node)?),
        }),
        Element::Sdf if painted => {
            if let Some(command) =
                field_command(scene, layout, node, bounds, transform, clip, color_overlay)?
            {
                list.commands.push(command);
            }
        }
        Element::Rect
        | Element::ClipRect
        | Element::Sdf
        | Element::Item
        | Element::Inset
        | Element::SdfShape
        | Element::MouseArea
        | Element::DropArea
        | Element::Row
        | Element::Column
        | Element::Grid
        | Element::Flickable
        | Element::Loader
        | Element::Timer
        | Element::Flex
        | Element::Custom => {}
    }
    let content_layer = if element == Element::ClipRect
        && scene.number(node, "border_width")? > 0.0
        && !scene.bool_value(node, "content_under_border")?
    {
        let border = scene.number(node, "border_width")?.max(0.0);
        let inner = Geometry {
            x: bounds.x + border,
            y: bounds.y + border,
            width: (bounds.width - border * 2.0).max(0.0),
            height: (bounds.height - border * 2.0).max(0.0),
        };
        let index = list.layers.len();
        list.layers.push(Layer {
            node,
            commands: list.commands.len()..list.commands.len(),
            parent: layer.or(inherited.layer),
            opacity: 1.0,
            blur: 0.0,
            shadow_color: Color::rgba8(0, 0, 0, 0),
            shadow_blur: 0.0,
            shadow_offset: [0.0, 0.0],
            shader: None,
            mask: Some(LayerMask {
                bounds: inner,
                transform,
                radii: radii.map(|radius| (radius - border).max(0.0)),
            }),
            alpha_mask: None,
            mask_for: None,
            // A layer's bounds are where it lands on the surface — the
            // scissor it is composited through — so they carry every
            // ancestor transform. The mask keeps the untransformed inner
            // rectangle beside the transform, as a mask does. Written
            // untransformed, a bordered ClipRect under anything moved by
            // `translate_y` composited through the place it would have
            // been, and its contents vanished.
            bounds: transform.bounds(inner),
        });
        Some((index, inner))
    } else {
        None
    };
    for &child in scene.paint_order(node)?.iter() {
        let child_clip = content_layer.map_or(clip, |(_, inner)| {
            let inner = transform.bounds(inner);
            Some(clip.map_or(inner, |clip| intersect_geometry(clip, inner)))
        });
        append_node(
            scene,
            layout,
            child,
            PaintContext {
                transform,
                clip: child_clip,
                overlay: color_overlay,
                layer: content_layer
                    .map(|(layer, _)| layer)
                    .or(layer)
                    .or(inherited.layer),
                // A field claims every shape beneath it, however deeply the
                // positioners nest, which is what lets an ordinary laid-out
                // row of rects arrive as one fused surface.
                in_field: inherited.in_field || element == Element::Sdf,
                // Text beneath inherits the nearest colour written above it;
                // one that says `inherit` itself passes the ancestor's on.
                color: match scene.current(node, "color") {
                    Ok(morf_scene::Value::Color(color)) => Some(*color),
                    _ => inherited.color,
                },
            },
            list,
        )?;
    }
    if let Some((content_layer, inner)) = content_layer {
        list.layers[content_layer].commands.end = list.commands.len();
        list.layers[content_layer].bounds = transform.bounds(inner);
    }
    if element == Element::ClipRect && scene.number(node, "border_width")? > 0.0 {
        list.commands.push(DrawCommand::Quad {
            node,
            bounds,
            transform,
            clip,
            color: Color::rgba8(0, 0, 0, 0),
            color_overlay: Color::rgba8(0, 0, 0, 0),
            gradient: None,
            radii,
            border_width: scene.number(node, "border_width")?,
            antialiasing: scene.bool_value(node, "antialiasing")?,
            border_pixel_aligned: scene.bool_value(node, "border_pixel_aligned")?,
            border_color: apply_overlay(scene.color_value(node, "border_color")?, color_overlay),
            blur: 0.0,
            shadow_color: Color::rgba8(0, 0, 0, 0),
            shadow_blur: 0.0,
            shadow_spread: 0.0,
            shadow_offset_x: 0.0,
            shadow_offset_y: 0.0,
            shadow_inner: false,
            // The border a ClipRect overlays is not the node's own fill, so it
            // carries no shader: the shader belongs to what is inside.
            shader: None,
        });
    }
    if let Some(layer) = layer {
        let start = list.layers[layer].commands.start;
        let end = list.commands.len();
        list.layers[layer].commands.end = end;
        let bounds =
            command_union(&list.commands[start..end]).unwrap_or_else(|| transform.bounds(bounds));
        let blurred = expand_geometry(bounds, layer_blur * 2.0);
        list.layers[layer].bounds = if layer_config.shadow_color.alpha > 0.0 {
            union_geometry(
                blurred,
                offset_geometry(
                    expand_geometry(bounds, layer_config.shadow_blur * 2.0),
                    layer_config.shadow_offset_x,
                    layer_config.shadow_offset_y,
                ),
            )
        } else {
            blurred
        };
    }
    if let (Some(source), Some(own)) = (alpha_mask, layer) {
        let host = outer_mask_layer.unwrap_or(own);
        if let Some(outer) = outer_mask_layer {
            let end = list.commands.len();
            list.layers[outer].commands.end = end;
            // What the inner layer reaches — its blur, its shadow — and not
            // only what its commands do.
            list.layers[outer].bounds = list.layers[own].bounds;
        }
        let invert = scene.bool_value(node, "mask_invert")?;
        let index = list.layers.len();
        list.layers.push(Layer {
            mask_for: Some(host),
            ..plain_layer(
                node,
                list.commands.len(),
                list.layers[host].parent,
                list.layers[host].bounds,
            )
        });
        match source {
            MaskSource::Gradient(gradient) => list
                .commands
                .push(gradient_mask(node, bounds, transform, clip, gradient)),
            MaskSource::Node(mask) => append_node(
                scene,
                layout,
                mask,
                PaintContext {
                    transform,
                    clip,
                    overlay: Color::rgba8(0, 0, 0, 0),
                    layer: Some(index),
                    in_field: false,
                    color: None,
                },
                list,
            )?,
        }
        list.layers[index].commands.end = list.commands.len();
        list.layers[host].alpha_mask = Some(AlphaMask {
            layer: index,
            invert,
        });
    }
    Ok(())
}
