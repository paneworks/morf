use morf_layout::{Geometry, Layout, Transform2D};
use morf_region::{BlendProfile, Operation, Shape};
use morf_scene::{Color, Element, NodeHandle, Scene, Value};

use crate::{commands::*, effects::*, sdf::*};

/// What a field hands down to every layer that does not speak for itself.
///
/// The container owns the compound: one blend, one fill, one position along the
/// morph. A layer opts out by naming its own.
#[derive(Clone, Copy)]
pub(crate) struct FieldDefaults {
    pub(crate) blend: f32,
    pub(crate) color: Color,
    pub(crate) morph: f32,
    /// The tint inherited from an ancestor.
    ///
    /// The field's own fill has always had this composited in; its layers did
    /// not, so tinting a subtree changed a field that fell back to the field
    /// colour and left alone every layer that named its own — half a field
    /// taking the tint and half of it ignoring it.
    pub(crate) overlay: Color,
    /// The shape of every smooth seam in the field.
    pub(crate) profile: BlendProfile,
    /// Where the field itself is drawn: its node and every ancestor
    /// transform. A layer that tracks another node is placed relative to it.
    pub(crate) transform: Transform2D,
}

/// Reads everything beneath a field that has a shape, in composition order.
///
/// This is what makes fields the foundation rather than a separate kind of
/// drawing: an ordinary `Rect` under an `Sdf` becomes a rounded-box layer, and
/// the walk descends through the positioners, so a `Row` of rects laid out by
/// the normal layout engine arrives here as a row of fields to fuse. Anything
/// without a field of its own — text, images, a mouse area — is left alone and
/// paints over the composition as usual.
///
/// A nested `Sdf` is not descended into. It is its own composition with its own
/// fill, and folding its layers into the parent would silently discard that.
pub(crate) fn field_layers(
    scene: &Scene,
    layout: &Layout,
    node: NodeHandle,
    defaults: FieldDefaults,
    layers: &mut Vec<SdfLayer>,
) -> Result<(), RenderError> {
    for &child in scene.children(node)? {
        // A mask is the field's mask, not one of its shapes.
        if !scene.bool_value(child, "visible")? || scene.is_mask(child) {
            continue;
        }
        match scene.element(child)? {
            Element::SdfShape => {
                if let Some(layer) = shape_layer(scene, layout, child, defaults)? {
                    layers.push(layer);
                }
            }
            Element::Rect | Element::ClipRect => {
                if let Some(layer) = rect_layer(scene, layout, child, defaults)? {
                    layers.push(layer);
                }
                // A rect may still position children, and those children are
                // part of the same composition.
                field_layers(scene, layout, child, defaults, layers)?;
            }
            Element::Sdf => {}
            _ => field_layers(scene, layout, child, defaults, layers)?,
        }
    }
    Ok(())
}

/// Whether a node's own paint is absorbed by an enclosing field.
///
/// A rect that became a layer must not also be drawn as a rect, or the
/// composition is painted twice — once fused and once with every seam back.
pub(crate) fn absorbed_by_field(element: Element) -> bool {
    matches!(
        element,
        Element::Rect | Element::ClipRect | Element::SdfShape
    )
}

/// Reads one `SdfShape` into a layer.
fn shape_layer(
    scene: &Scene,
    layout: &Layout,
    node: NodeHandle,
    defaults: FieldDefaults,
) -> Result<Option<SdfLayer>, RenderError> {
    let own_rotation = scene.number(node, "rotation")? as f32;
    let own_matrix = layer_matrix(scene, node)?;
    let own_radii = rect_radii(scene, node)?.map(|radius| radius as f32);
    // A tracking layer is wherever its node is drawn; any other is where its
    // own layout put it.
    let placement = match scene.track(node) {
        Some(target) => tracked_placement(
            scene,
            layout,
            target,
            defaults.transform,
            TrackedLayer {
                rotation: own_rotation,
                matrix: own_matrix,
                radii: own_radii,
            },
        )?,
        None => layout.geometry(node).map(|bounds| Placement {
            bounds,
            rotation: own_rotation,
            matrix: own_matrix,
            radii: own_radii,
        }),
    };
    let Some(Placement {
        bounds,
        rotation,
        matrix,
        radii,
    }) = placement
    else {
        return Ok(None);
    };
    // A named letter decides the family: it is one particular outline, not a
    // shape with parameters, so there is nothing for `shape` to say about it.
    let glyph = scene.string_value(node, "glyph")?.chars().next();
    let glyph_morph_to = scene.string_value(node, "glyph_morph_to")?.chars().next();
    // And a named drawing does the same. Read as a borrow first and only
    // allocated when there is one, so a plain shape pays nothing per frame.
    let svg_source = match scene.string_value(node, "source")? {
        "" => None,
        source => Some(source),
    };
    let name = scene.string_value(node, "shape")?;
    let named = Shape::parse(name)
        .ok_or_else(|| RenderError::Scene(format!("unknown SdfShape shape `{name}`")))?;
    // Naming a letter is enough to mean the layer *is* that letter, unless the
    // shape was named too — which is how a shape morphs into a letter rather
    // than out of one.
    let shape = if (glyph.is_some() || svg_source.is_some()) && name == "circle" {
        Shape::Polygon
    } else {
        named
    };
    let target = scene.string_value(node, "morph_to")?;
    let morph_to = if target.is_empty() {
        shape
    } else {
        Shape::parse(target)
            .ok_or_else(|| RenderError::Scene(format!("unknown SdfShape shape `{target}`")))?
    };
    let operation = scene.string_value(node, "operation")?;
    let operation = Operation::parse(operation)
        .ok_or_else(|| RenderError::Scene(format!("unknown SdfShape operation `{operation}`")))?;
    Ok(Some(SdfLayer {
        glyph,
        glyph_morph_to,
        svg_source: svg_source.map(Into::into),
        svg_source_morph_to: match scene.string_value(node, "source_morph_to")? {
            "" => None,
            source => Some(source.into()),
        },
        // Only a letter has a face. Asking for the string when there is no
        // glyph would allocate on every plain shape in every field, every
        // frame, to describe something that is never read.
        font_family: match glyph {
            Some(_) => Some(scene.string_value(node, "font_family")?.into()),
            None => None,
        },
        font_family_morph_to: match glyph {
            Some(_) => match scene.string_value(node, "font_family_morph_to")? {
                "" => None,
                named => Some(named.into()),
            },
            None => None,
        },
        bounds,
        color: layer_color(scene, node, defaults)?,
        shape,
        morph_to,
        morph: {
            let own = scene.number(node, "morph_progress")?;
            if own < 0.0 {
                defaults.morph
            } else {
                own as f32
            }
        }
        .clamp(0.0, 1.0),
        operation,
        blend: layer_blend(scene, node, defaults.blend)?,
        rotation,
        matrix,
        blend_group: scene.number(node, "blend_group")?.clamp(0.0, 65_535.0) as u32,
        profile: defaults.profile,
        radii,
        points: scene.number(node, "points")?.clamp(3.0, 64.0) as f32,
        inner_radius: scene.number(node, "inner_radius")?.clamp(0.01, 1.0) as f32,
        thickness: scene.number(node, "thickness")?.max(0.0) as f32,
        angle: scene.number(node, "angle")?.clamp(0.0, 360.0) as f32,
    }))
}

/// Reads an ordinary rect into a rounded-box layer.
///
/// A field box carries one corner radius where a rect carries four, so the
/// largest of them is used: a rect that is round on one corner reads as round
/// rather than square once it is part of a fused surface.
fn rect_layer(
    scene: &Scene,
    layout: &Layout,
    node: NodeHandle,
    defaults: FieldDefaults,
) -> Result<Option<SdfLayer>, RenderError> {
    let Some(bounds) = layout.geometry(node) else {
        return Ok(None);
    };
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return Ok(None);
    }
    let blend = layer_blend(scene, node, defaults.blend)?;
    Ok(Some(SdfLayer {
        glyph: None,
        glyph_morph_to: None,
        font_family: None,
        svg_source: None,
        svg_source_morph_to: None,
        font_family_morph_to: None,
        bounds,
        // A rect brings its own colour into the composition, so a fused row of
        // differently coloured rects keeps every one of them and blends across
        // the seams rather than flattening to a single fill.
        color: scene
            .color_value(node, "color")
            .map_or(defaults.color, |color| {
                apply_overlay(color, defaults.overlay)
            }),
        shape: Shape::Box,
        morph_to: Shape::Box,
        morph: 0.0,
        // A rect joins what is already there, smoothly when the field asks for
        // it. Nothing about the rect had to be written differently to take
        // part; it is the container that decides.
        operation: if blend > 0.0 {
            Operation::SmoothUnion
        } else {
            Operation::Union
        },
        blend,
        rotation: scene.number(node, "rotation")? as f32,
        matrix: IDENTITY_LINEAR,
        blend_group: 0,
        profile: defaults.profile,
        // All four, so a rect rounded on one edge keeps that shape once it is
        // part of a fused surface instead of collapsing to a single radius.
        radii: rect_radii(scene, node)?.map(|radius| radius as f32),
        points: 5.0,
        inner_radius: 0.5,
        thickness: 0.0,
        angle: 90.0,
    }))
}

/// A layer's own fill, falling back to the field's when it names none.
///
/// Transparent is the sentinel rather than a missing property, so a layer that
/// wants the field's colour simply says nothing.
fn layer_color(
    scene: &Scene,
    node: NodeHandle,
    defaults: FieldDefaults,
) -> Result<Color, RenderError> {
    let own = scene.color_value(node, "fill_color")?;
    // The default already carries the overlay; a layer's own colour has to have
    // it applied here, so both answers are tinted the same way.
    Ok(if own.alpha > 0.0 {
        apply_overlay(own, defaults.overlay)
    } else {
        defaults.color
    })
}

/// A layer's own seam radius, falling back to the field's.
fn layer_blend(scene: &Scene, node: NodeHandle, default_blend: f32) -> Result<f32, RenderError> {
    let own = scene.number(node, "blend").unwrap_or(0.0).max(0.0) as f32;
    Ok(if own > 0.0 { own } else { default_blend })
}

/// The identity as a column-major 2×2.
pub(crate) const IDENTITY_LINEAR: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

/// Where a layer sits in its field and how it is bent.
struct Placement {
    bounds: Geometry,
    rotation: f32,
    matrix: [f32; 4],
    radii: [f32; 4],
}

/// What a tracking layer adds of its own on top of its node's placement.
struct TrackedLayer {
    rotation: f32,
    matrix: [f32; 4],
    radii: [f32; 4],
}

/// A layer's own `matrix`, or the identity.
fn layer_matrix(scene: &Scene, node: NodeHandle) -> Result<[f32; 4], RenderError> {
    Ok(match scene.current(node, "matrix")? {
        Value::List(items) if items.len() == 4 => {
            let mut matrix = IDENTITY_LINEAR;
            for (slot, item) in matrix.iter_mut().zip(items) {
                if let Value::Number(value) = item {
                    *slot = *value as f32;
                }
            }
            matrix
        }
        _ => IDENTITY_LINEAR,
    })
}

/// Where `target` is drawn, as a layer in the field drawn through `field`.
///
/// The target's layout box through every transform above it and its own —
/// animated values, a stretch, a matrix — brought into the field's own space.
/// A scale along the axes becomes the layer's size, so a stretched panel is
/// still an exact rounded box with round corners; anything else (a turn, a
/// shear, a stretch along a diagonal) rides in the layer's matrix.
///
/// Nothing when the target is hidden, or not laid out in this surface.
fn tracked_placement(
    scene: &Scene,
    layout: &Layout,
    target: NodeHandle,
    field: Transform2D,
    own: TrackedLayer,
) -> Result<Option<Placement>, RenderError> {
    let Some(geometry) = layout.geometry(target) else {
        return Ok(None);
    };
    let mut current = Some(target);
    while let Some(node) = current {
        if !scene.bool_value(node, "visible")? {
            return Ok(None);
        }
        current = scene.parent(node)?;
    }
    let Ok(drawn) = layout.chain_transform(scene, target) else {
        return Ok(None);
    };
    let Some(field_inverse) = field.inverse() else {
        return Ok(None);
    };
    let placed = field_inverse.then(drawn);
    let centre = placed.point(
        geometry.x + geometry.width / 2.0,
        geometry.y + geometry.height / 2.0,
    );
    let [a, b, c, d, _, _] = placed.matrix;
    // The node's map, then the layer's own turn and matrix inside it.
    let linear = multiply(
        [a as f32, b as f32, c as f32, d as f32],
        multiply(rotation_matrix(own.rotation), own.matrix),
    );
    let axis_aligned = linear[1].abs() < 1e-6 && linear[2].abs() < 1e-6;
    let (size, matrix, radii) = if axis_aligned && linear[0] > 0.0 && linear[3] > 0.0 {
        let (sx, sy) = (f64::from(linear[0]), f64::from(linear[3]));
        (
            (geometry.width * sx, geometry.height * sy),
            IDENTITY_LINEAR,
            own.radii.map(|radius| radius * linear[0].min(linear[3])),
        )
    } else {
        ((geometry.width, geometry.height), linear, own.radii)
    };
    Ok(Some(Placement {
        bounds: Geometry {
            x: centre.0 - size.0 / 2.0,
            y: centre.1 - size.1 / 2.0,
            width: size.0,
            height: size.1,
        },
        rotation: 0.0,
        matrix,
        radii,
    }))
}

/// `left * right`, both column major.
fn multiply(left: [f32; 4], right: [f32; 4]) -> [f32; 4] {
    [
        left[0] * right[0] + left[2] * right[1],
        left[1] * right[0] + left[3] * right[1],
        left[0] * right[2] + left[2] * right[3],
        left[1] * right[2] + left[3] * right[3],
    ]
}

/// A clockwise turn by `degrees` on a surface whose `y` grows downwards, as
/// the field's own `rotation` turns a layer.
fn rotation_matrix(degrees: f32) -> [f32; 4] {
    if degrees == 0.0 {
        return IDENTITY_LINEAR;
    }
    let (sin, cos) = degrees.to_radians().sin_cos();
    [cos, sin, -sin, cos]
}
