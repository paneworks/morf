//! Where a texture lands: its placement in its box, the field uniforms it is
//! sampled with, and the instance that draws it.

use crate::effects::color_array;
use crate::{DistanceFieldStyle, ImageFillMode};
use morf_layout::{Geometry, Transform2D};

use super::super::glyphs::*;
use super::{TextureBatch, TextureImage, TexturePlacement, TextureStyle};

pub(crate) fn texture_placement(
    bounds: Geometry,
    intrinsic: (u32, u32),
    fill_mode: ImageFillMode,
    transform: Transform2D,
) -> TexturePlacement {
    let source_width = f64::from(intrinsic.0.max(1));
    let source_height = f64::from(intrinsic.1.max(1));
    match fill_mode {
        ImageFillMode::Stretch => TexturePlacement {
            bounds,
            transform,
            logical_width: bounds.width.ceil().max(1.0) as u32,
            logical_height: bounds.height.ceil().max(1.0) as u32,
            uv: [0.0, 0.0, 1.0, 1.0],
        },
        ImageFillMode::PreserveAspectFit => {
            let scale = (bounds.width / source_width).min(bounds.height / source_height);
            let width = source_width * scale;
            let height = source_height * scale;
            TexturePlacement {
                bounds: Geometry {
                    x: bounds.x + (bounds.width - width) / 2.0,
                    y: bounds.y + (bounds.height - height) / 2.0,
                    width,
                    height,
                },
                transform,
                logical_width: width.ceil().max(1.0) as u32,
                logical_height: height.ceil().max(1.0) as u32,
                uv: [0.0, 0.0, 1.0, 1.0],
            }
        }
        ImageFillMode::PreserveAspectCrop => {
            let scale = (bounds.width / source_width).max(bounds.height / source_height);
            let width = source_width * scale;
            let height = source_height * scale;
            let uv_width = (bounds.width / width) as f32;
            let uv_height = (bounds.height / height) as f32;
            TexturePlacement {
                bounds,
                transform,
                logical_width: width.ceil().max(1.0) as u32,
                logical_height: height.ceil().max(1.0) as u32,
                uv: [
                    (1.0 - uv_width) / 2.0,
                    (1.0 - uv_height) / 2.0,
                    uv_width,
                    uv_height,
                ],
            }
        }
    }
}

/// Converts the field style into the units the shader samples in.
///
/// The cached texture maps `[-spread, spread]` source pixels onto `[0, 1]`, so
/// a width expressed in pixels has to be divided by the full span to land in
/// the same space as the sampled value.
/// The same uniform for a glyph, whose field was measured at a fixed size.
///
/// A glyph's spread is in reference pixels, not in the pixels it is drawn at,
/// so an outline asked for in logical pixels has to be converted through the
/// ratio between the two. Doing it here rather than in the configuration is
/// what lets an outline width mean the same thing at every font size.
/// Extra edge outset for small text, in logical pixels.
///
/// A hinted rasterizer snaps a stem onto the pixel grid, so a one-pixel stem is
/// one solid pixel. A field has no hinting: the same stem lands wherever the
/// outline puts it, usually spread across two pixels at part strength each, and
/// the letter reads lighter than the hinted one it replaced. Moving the edge out
/// by a fraction of a pixel gives that back.
///
/// Only where it is the problem. Above the fade the stems are wide enough that
/// the grid no longer decides how solid they look, and the same outset there
/// would simply be a heavier font than the one asked for.
fn hinting_bias(size: f64) -> f32 {
    const FULL_BELOW: f64 = 10.0;
    const NONE_ABOVE: f64 = 20.0;
    const OUTSET: f32 = 0.18;
    let reach = ((NONE_ABOVE - size) / (NONE_ABOVE - FULL_BELOW)).clamp(0.0, 1.0);
    OUTSET * reach as f32
}

pub(crate) fn glyph_field_uniform(style: DistanceFieldStyle, size: f64) -> [f32; 4] {
    // How much of the field one logical pixel covers at this size. Asked for
    // rather than derived here: the spread is capped, so it is no longer a
    // fixed fraction of the reference and a second copy of the arithmetic would
    // disagree with the first.
    let per_pixel = morf_text::field_units_per_logical_px(size.max(1.0) as f32);
    [
        // Positive thickness moves the edge outwards, which is the direction
        // that adds ink — the field counts upwards away from the glyph.
        0.5 + (style.thickness + hinting_bias(size)) * per_pixel,
        style.softness * per_pixel,
        style.outline_width * per_pixel,
        0.0,
    ]
}

pub(crate) fn distance_field_uniform(style: DistanceFieldStyle, spread: f32) -> [f32; 4] {
    let span = (spread.max(0.5) * 2.0).max(f32::EPSILON);
    [
        // The same neutral edge and the same signed offset the glyph path
        // uses. This used to pass `weight` through as an absolute threshold,
        // so one struct field meant two different things depending on which
        // producer had filled it in.
        0.5 + style.thickness / span,
        style.softness / span,
        style.outline_width / span,
        0.0,
    ]
}

pub(crate) fn push_texture_instance(
    batch: &mut TextureBatch,
    command_index: usize,
    image: TextureImage,
    placement: TexturePlacement,
    style: TextureStyle,
    target_size: (u32, u32),
    scale: f64,
) {
    let bounds = placement.bounds;
    let (origin, axes) = transformed_quad(placement.transform, bounds, scale, target_size);
    batch.command_instances[command_index] = Some(batch.instances.len() as u32);
    batch.instances.push(GlyphInstance {
        origin,
        axes,
        uv: placement.uv,
        color: [1.0, 1.0, 1.0, 1.0],
        color_overlay: color_array(style.overlay),
        mode: [
            0.0,
            0.0,
            f32::from(style.distance_field),
            f32::from(style.distance_field),
        ],
        field: distance_field_uniform(style.field, style.spread),
        outline_color: color_array(style.field.outline_color),
        ..GlyphInstance::default()
    });
    batch.images.push(image);
}
