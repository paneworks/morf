//! Paint damage: what a field's new colours change when nothing else did.

use super::*;

impl DrawCommand {
    /// What changed between two pictures of one field that differ only in
    /// colour: the fill, the outline, a layer's own colour, the gradient, the
    /// overlay or the shadow's tint. `None` when anything else changed, and
    /// the whole command is damaged as usual.
    ///
    /// A recolour changes every pixel the field covers and none it does not.
    /// A fullscreen frame with its opening subtracted covers four strips round
    /// the edge, so a new scheme fading in repaints those and not the eight
    /// million pixels of the opening the frame was cut from.
    pub(crate) fn field_paint_changed(&self, old: &Self) -> Option<Vec<Geometry>> {
        let (
            Self::Field {
                transform,
                clip,
                stroke_width,
                softness,
                shadow_color,
                shadow_blur,
                shadow_spread,
                shadow_offset_x,
                shadow_offset_y,
                shader,
                layers,
                ..
            },
            Self::Field {
                shadow_color: old_shadow,
                ..
            },
        ) = (self, old)
        else {
            return None;
        };
        if shader.is_some() || layers.len() > crate::field::MAX_FIELD_LAYERS {
            return None;
        }
        if repainted(old, self)? != *self {
            return None;
        }
        let margin = crate::field::field_spread(*stroke_width, *softness, layers) + 2.0;
        let mut covered = coverage(layers, margin)?;
        if shadow_color.alpha > 0.0 || old_shadow.alpha > 0.0 {
            let grown = shadow_blur.max(0.0) + shadow_spread.abs();
            let cast: Vec<Geometry> = covered
                .iter()
                .map(|area| {
                    crate::effects::offset_geometry(
                        crate::effects::expand_geometry(*area, grown),
                        *shadow_offset_x,
                        *shadow_offset_y,
                    )
                })
                .collect();
            covered.extend(cast);
        }
        Some(
            covered
                .into_iter()
                .map(|area| transform.bounds(area))
                .map(|area| clip.map_or(area, |clip| intersect_geometry(area, clip)))
                .collect(),
        )
    }
}

/// `old` with the colours of `new`: equal to `new` exactly when colour is all
/// that differs.
fn repainted(old: &DrawCommand, new: &DrawCommand) -> Option<DrawCommand> {
    let mut same = old.clone();
    let (
        DrawCommand::Field {
            fill_color,
            stroke_color,
            gradient,
            color_overlay,
            shadow_color,
            layers,
            ..
        },
        DrawCommand::Field {
            fill_color: new_fill,
            stroke_color: new_stroke,
            gradient: new_gradient,
            color_overlay: new_overlay,
            shadow_color: new_shadow,
            layers: new_layers,
            ..
        },
    ) = (&mut same, new)
    else {
        return None;
    };
    if layers.len() != new_layers.len() {
        return None;
    }
    *fill_color = *new_fill;
    *stroke_color = *new_stroke;
    gradient.clone_from(new_gradient);
    *color_overlay = *new_overlay;
    *shadow_color = *new_shadow;
    for (layer, new_layer) in layers.iter_mut().zip(new_layers) {
        layer.color = new_layer.color;
    }
    Some(same)
}

/// The rectangles a composition can paint, each widened by `margin`: every
/// layer that adds to it, less the solid inside of every box taken from it.
/// `None` for an intersection, which can take away anywhere.
fn coverage(layers: &[SdfLayer], margin: f64) -> Option<Vec<Geometry>> {
    let mut covered: Vec<Geometry> = Vec::new();
    for (index, layer) in layers.iter().enumerate() {
        match layer.operation {
            Operation::Intersect | Operation::SmoothIntersect => return None,
            Operation::Subtract | Operation::SmoothSubtract if index > 0 => {
                for hole in solid_inside(layer, margin) {
                    covered = covered
                        .into_iter()
                        .flat_map(|area| subtract(area, hole))
                        .collect();
                }
            }
            _ => {
                if let Some(reach) =
                    crate::field::field_reach(0.0, 0.0, std::slice::from_ref(layer))
                {
                    covered.push(crate::effects::expand_geometry(reach, margin));
                }
            }
        }
    }
    Some(covered)
}

/// Where a subtracted box takes away everything for certain, as the two bars
/// of a cross: its rectangle less the seam, edge and softness along each edge,
/// and less the corner radius across the other way, so only the rounded
/// corners are left uncertain. Only a plain, whole, upright box; any other
/// shape takes nothing.
fn solid_inside(layer: &SdfLayer, margin: f64) -> Vec<Geometry> {
    let plain = layer.shape == Shape::Box
        && (layer.morph <= 0.0 || layer.morph_to == Shape::Box)
        && layer.opacity >= 1.0
        && layer.rotation == 0.0
        && layer.matrix == [1.0, 0.0, 0.0, 1.0]
        && layer.glyph.is_none()
        && layer.svg_source.is_none();
    if !plain {
        return Vec::new();
    }
    let corner = f64::from(layer.radii.iter().copied().fold(0.0_f32, f32::max));
    let bounds = layer.bounds;
    let inset = |across: f64, down: f64| Geometry {
        x: bounds.x + across,
        y: bounds.y + down,
        width: bounds.width - 2.0 * across,
        height: bounds.height - 2.0 * down,
    };
    [
        inset(margin, margin + corner),
        inset(margin + corner, margin),
    ]
    .into_iter()
    .filter(|bar| bar.width > 0.0 && bar.height > 0.0)
    .collect()
}

/// `area` without `hole`: up to four rectangles, the bands above and below
/// the hole and the pieces either side of it.
fn subtract(area: Geometry, hole: Geometry) -> Vec<Geometry> {
    let (left, top) = (area.x, area.y);
    let (right, bottom) = (area.x + area.width, area.y + area.height);
    let hole_left = hole.x.max(left);
    let hole_top = hole.y.max(top);
    let hole_right = (hole.x + hole.width).min(right);
    let hole_bottom = (hole.y + hole.height).min(bottom);
    if hole_left >= hole_right || hole_top >= hole_bottom {
        return vec![area];
    }
    let band = |x: f64, y: f64, x2: f64, y2: f64| Geometry {
        x,
        y,
        width: x2 - x,
        height: y2 - y,
    };
    [
        band(left, top, right, hole_top),
        band(left, hole_bottom, right, bottom),
        band(left, hole_top, hole_left, hole_bottom),
        band(hole_right, hole_top, right, hole_bottom),
    ]
    .into_iter()
    .filter(|piece| piece.width > 0.0 && piece.height > 0.0)
    .collect()
}

impl DrawCommand {
    /// What changed between two pictures of one path that differ only in
    /// colour: where its outline is drawn, and not the whole node it sits in.
    /// A row of ruler ticks along one edge of a fullscreen frame is a strip
    /// ten pixels deep, recoloured or not.
    pub(crate) fn path_paint_changed(&self, old: &Self) -> Option<Vec<Geometry>> {
        let (
            Self::Path {
                bounds,
                transform,
                clip,
                color_overlay,
                paint,
                ..
            },
            Self::Path {
                paint: old_paint, ..
            },
        ) = (self, old)
        else {
            return None;
        };
        let mut same = old.clone();
        if let Self::Path {
            color_overlay: same_overlay,
            paint: same_paint,
            ..
        } = &mut same
        {
            *same_overlay = *color_overlay;
            same_paint.fill_color = paint.fill_color;
            same_paint.stroke_color = paint.stroke_color;
        }
        if same != *self {
            return None;
        }
        // A stroke that appears or goes reaches further than either colour.
        let logical = (bounds.width, bounds.height);
        let margin = paint
            .margin(logical.0, logical.1)
            .max(old_paint.margin(logical.0, logical.1));
        let mut outlines = crate::path::PathOutlines::default();
        let drawn = crate::path::drawn_extent(&mut outlines, paint, logical, margin)?;
        let area = transform.bounds(Geometry {
            x: bounds.x + drawn.x,
            y: bounds.y + drawn.y,
            width: drawn.width,
            height: drawn.height,
        });
        Some(vec![
            clip.map_or(area, |clip| intersect_geometry(area, clip)),
        ])
    }

    /// What changed between two pictures of one outlined rectangle that
    /// differ only in its border's colour: the border, round the edge, and
    /// not the empty inside it.
    pub(crate) fn quad_border_changed(&self, old: &Self) -> Option<Vec<Geometry>> {
        let (
            Self::Quad {
                bounds,
                transform,
                clip,
                color,
                color_overlay,
                gradient,
                radii,
                border_width,
                border_color,
                blur,
                shadow_color,
                shader,
                ..
            },
            Self::Quad { .. },
        ) = (self, old)
        else {
            return None;
        };
        let hollow = color.alpha <= 0.0
            && color_overlay.alpha <= 0.0
            && gradient.is_none()
            && shadow_color.alpha <= 0.0
            && shader.is_none()
            && *border_width > 0.0;
        if !hollow {
            return None;
        }
        let mut same = old.clone();
        if let Self::Quad {
            border_color: same_border,
            ..
        } = &mut same
        {
            *same_border = *border_color;
        }
        if same != *self {
            return None;
        }
        // The border, its rounded corners' reach, the blur and a pixel of
        // antialiasing either side.
        let corner = radii.iter().copied().fold(0.0, f64::max);
        let depth = border_width + corner + blur.max(0.0) + 2.0;
        let outer = crate::effects::expand_geometry(*bounds, 1.0);
        let inner = crate::effects::expand_geometry(*bounds, -depth);
        if inner.width <= 0.0 || inner.height <= 0.0 {
            return None;
        }
        Some(
            subtract(outer, inner)
                .into_iter()
                .map(|area| transform.bounds(area))
                .map(|area| clip.map_or(area, |clip| intersect_geometry(area, clip)))
                .collect(),
        )
    }
}
