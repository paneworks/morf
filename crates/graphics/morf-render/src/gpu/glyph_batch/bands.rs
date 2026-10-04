//! Solid bands drawn with the glyphs: decoration lines, a text input's
//! selection and caret.

use crate::TextEdit;
use crate::effects::color_array;
use morf_layout::{Geometry, Transform2D};
use morf_scene::{Color, DecorationLine, TextDecoration};
use morf_text::{LineBand, TextSystem};

use super::super::glyphs::*;

/// A solid quad through the glyph pipeline: a decoration, a selection, a
/// caret. Clipped and masked with the command it belongs to.
pub(super) fn push_band(
    instances: &mut Vec<GlyphInstance>,
    command_spans: &mut [Vec<GlyphSpan>],
    band: PreparedBand,
    scale: f32,
    target: (u32, u32),
) {
    let (origin, axes) = transformed_quad(band.transform, band.rect, f64::from(scale), target);
    let instance = instances.len() as u32;
    let spans = &mut command_spans[band.command_index];
    if let Some(span) = spans.last_mut()
        && !span.color
        && span.range.end == instance
    {
        span.range.end = instance + 1;
    } else {
        spans.push(GlyphSpan {
            range: instance..instance + 1,
            color: false,
            lcd: false,
        });
    }
    instances.push(GlyphInstance {
        origin,
        axes,
        color: color_array(band.color),
        color_overlay: color_array(band.color_overlay),
        // `z` past one is solid: no sample, the colour as it is.
        mode: [0.0, 0.0, 2.0, 0.0],
        ..GlyphInstance::default()
    });
}

/// A text input's selection rectangles and its caret, in surface space.
///
/// Read off the same shaped buffer the glyphs just came from, so the caret
/// stands exactly where a letter would be typed.
#[allow(clippy::too_many_arguments)]
pub(super) fn edit_bands(
    text_system: &TextSystem,
    node: morf_scene::NodeHandle,
    edit: &TextEdit,
    alignment: morf_layout::TextAlignment,
    content: Geometry,
    color_overlay: Color,
    transform: Transform2D,
    command_index: usize,
) -> (Vec<PreparedBand>, Option<PreparedBand>) {
    let map = text_system.caret_map(node).unwrap_or_default();
    let selection = if edit.placeholder || edit.selection.is_empty() {
        Vec::new()
    } else {
        map.selection(edit.selection.start, edit.selection.end)
            .into_iter()
            .map(|span| PreparedBand {
                rect: Geometry {
                    x: content.x + f64::from(span.x),
                    y: content.y + f64::from(span.y),
                    width: f64::from(span.width),
                    height: f64::from(span.height),
                },
                color: edit.selection_color,
                color_overlay,
                transform,
                command_index,
            })
            .collect()
    };
    let caret = edit.caret.map(|offset| {
        let mut caret = map.caret(offset);
        // The placeholder is not what the caret is in: it stands where the
        // first letter typed would, which the alignment decides.
        if edit.placeholder {
            caret.x = match alignment {
                morf_layout::TextAlignment::Center => content.width as f32 / 2.0,
                morf_layout::TextAlignment::Right => content.width as f32,
                _ => 0.0,
            };
        }
        let width = edit.caret_width;
        // Centred on the boundary, but never hanging off the left of the
        // text, where the first letter's caret would be half clipped away.
        let left = (f64::from(caret.x) - width / 2.0).max(f64::from(caret.x).min(0.0));
        PreparedBand {
            rect: Geometry {
                x: content.x + left,
                y: content.y + f64::from(caret.y),
                width,
                height: f64::from(caret.height),
            },
            color: edit.caret_color,
            color_overlay,
            transform,
            command_index,
        }
    });
    (selection, caret)
}

/// One decoration line, positioned, waiting to become an instance.
pub(crate) struct PreparedBand {
    pub(crate) rect: Geometry,
    pub(crate) color: Color,
    pub(crate) color_overlay: Color,
    pub(crate) transform: Transform2D,
    pub(crate) command_index: usize,
}

/// Where a decoration runs along each line, from the face's own metrics.
#[allow(clippy::too_many_arguments)]
pub(super) fn decoration_bands(
    lines: Vec<LineBand>,
    decoration: &TextDecoration,
    size: f64,
    bounds: Geometry,
    text_color: Color,
    color_overlay: Color,
    transform: Transform2D,
    command_index: usize,
    scale: f32,
) -> Vec<PreparedBand> {
    let color = decoration.color.unwrap_or(text_color);
    let scale = f64::from(scale.max(f32::EPSILON));
    lines
        .into_iter()
        .map(|line| {
            let thickness = decoration
                .thickness
                .unwrap_or(f64::from(line.stroke_size))
                .max(size / 24.0);
            let baseline = bounds.y + f64::from(line.baseline);
            // Where the face puts the line, then the configuration's offset,
            // downwards; the band's own thickness is centred on that.
            let centre = match decoration.line {
                DecorationLine::Under => {
                    baseline + f64::from(line.underline_offset) + thickness / 2.0
                }
                DecorationLine::Over => baseline - f64::from(line.ascent) + thickness / 2.0,
                DecorationLine::Through => baseline - f64::from(line.strikeout_offset),
            } + decoration.offset;
            let (y, height) = pixel_band(centre, thickness, scale);
            PreparedBand {
                rect: Geometry {
                    x: bounds.x + f64::from(line.x),
                    y,
                    width: f64::from(line.width),
                    height,
                },
                color,
                color_overlay,
                transform,
                command_index,
            }
        })
        .collect()
}

/// A band `thickness` tall around `centre`, as whole device pixels: at least
/// one, starting on a pixel edge. Returns its top and height, logical.
///
/// A solid quad is not antialiased; the rasteriser fills the pixels whose
/// centres it covers. A face's strikeout at a small size is under a pixel
/// thick, and a band thinner than a pixel covers a pixel centre only when it
/// happens to straddle one: the same line drawn at y = 10.3 showed and at
/// y = 10.6 vanished, so one card in a list lost its strike-through while
/// its neighbours, a fraction of a pixel away, kept theirs.
pub(crate) fn pixel_band(centre: f64, thickness: f64, scale: f64) -> (f64, f64) {
    let pixels = (thickness * scale).round().max(1.0);
    let top = (centre * scale - pixels / 2.0).round();
    (top / scale, pixels / scale)
}

#[cfg(test)]
mod tests {
    use super::pixel_band;

    #[test]
    fn a_thin_band_covers_a_whole_pixel_wherever_it_falls() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for step in 0..40 {
                let centre = 10.0 + f64::from(step) * 0.025;
                let (top, height) = pixel_band(centre, 0.7, scale);
                let (top, height) = (top * scale, height * scale);
                assert!(
                    (top - top.round()).abs() < 1e-9 && height >= 1.0 - 1e-9,
                    "at {centre} x{scale}: {top} + {height}"
                );
                // Still where the face put it: within a pixel of the centre.
                assert!(((top + height / 2.0) - centre * scale).abs() <= 1.0);
            }
        }
        // A band already whole pixels thick keeps its thickness.
        assert_eq!(pixel_band(20.0, 2.0, 1.0), (19.0, 2.0));
    }
}
