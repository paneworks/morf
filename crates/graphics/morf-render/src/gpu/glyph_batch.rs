use crate::effects::color_array;
use crate::{DrawCommand, DrawList, VerticalAlignment};
use morf_layout::{Geometry, TextMeasurer, TextOptions};
use morf_scene::{Color, DecorationLine, TextDecoration};
use morf_text::{RasterContent, RasterGlyph, TextSystem};

use super::{backend_types::*, glyphs::*, textures::*};

mod bands;

pub(crate) use bands::PreparedBand;
use bands::{decoration_bands, edit_bands, push_band};

// Shaped text on its way to the screen: one batch of glyph quads per frame,
// drawn from the atlas the glyphs were measured into.

pub(crate) struct GlyphBatchContext<'a> {
    pub(crate) queue: &'a wgpu::Queue,
    pub(crate) mask_atlas: &'a mut GlyphAtlas,
    pub(crate) color_atlas: &'a mut GlyphAtlas,
    pub(crate) target_size: (u32, u32),
}

pub(crate) fn create_glyph_batch(
    context: GlyphBatchContext<'_>,
    text_system: &mut TextSystem,
    list: &DrawList,
    scale_120: u32,
) -> Result<Option<GlyphBatch>, GpuError> {
    let GlyphBatchContext {
        queue,
        mask_atlas,
        color_atlas,
        target_size: (target_width, target_height),
    } = context;
    let scale = scale_120.max(1) as f32 / 120.0;
    let mut glyphs = Vec::new();
    let mut bands: Vec<PreparedBand> = Vec::new();
    // Drawn before any glyph, so a selection lies under the text it selects.
    let mut under: Vec<PreparedBand> = Vec::new();
    for (command_index, command) in list.commands.iter().enumerate() {
        if let DrawCommand::Terminal {
            bounds,
            transform,
            color_overlay,
            screen,
            ..
        } = command
        {
            super::terminal_batch::prepare_terminal(
                text_system,
                screen,
                *bounds,
                *transform,
                *color_overlay,
                command_index,
                scale,
                super::terminal_batch::TerminalOut {
                    glyphs: &mut glyphs,
                    under: &mut under,
                    over: &mut bands,
                },
            );
            continue;
        }
        let DrawCommand::Text {
            node,
            bounds,
            transform,
            text,
            family,
            font_source,
            size,
            font_weight,
            color,
            color_overlay,
            wrap,
            elide,
            max_lines,
            horizontal_alignment,
            vertical_alignment,
            field_style,
            morph_to,
            morph_progress,
            style,
            decoration,
            edit,
            ..
        } = command
        else {
            continue;
        };
        let measured = text_system.measure(
            *node,
            text,
            family,
            *size,
            TextOptions {
                width: Some(bounds.width),
                wrap: *wrap,
                alignment: *horizontal_alignment,
                elide: *elide,
                font_weight: *font_weight,
                font_source: (!font_source.is_empty()).then(|| font_source.clone()),
                max_lines: *max_lines,
                style: style.clone(),
            },
        );
        let spare_height = (bounds.height - measured.height).max(0.0);
        let vertical_offset = match vertical_alignment {
            VerticalAlignment::Top => 0.0,
            VerticalAlignment::Center => spare_height / 2.0,
            VerticalAlignment::Bottom => spare_height,
        };
        // Every glyph that has an outline is drawn from its field. There used
        // to be a size below which a direct rasterization won, and it won for a
        // reason that has since been removed: the field was measured at one
        // size for all text, so small text read a sixty-four pixel field
        // through eleven pixels and came back scarred by the minification. A
        // field is now measured at a reference chosen from the size it will be
        // drawn at, so it is never read at worse than half its own resolution.
        //
        // What that buys is one representation for all text: a size can be
        // animated without refilling the atlas, and thickness, softness and an
        // outline are thresholds every label can ask for rather than a
        // privilege of large ones.
        let morphing = *morph_progress > 0.0 && !morph_to.is_empty();
        // A text input's content moves under its box as it scrolls; the box,
        // and so the clip, stays where it is.
        let (scroll_x, scroll_y) = edit.as_ref().map_or((0.0, 0.0), |edit| edit.scroll);
        let content = Geometry {
            x: bounds.x - scroll_x,
            y: bounds.y + vertical_offset - scroll_y,
            width: bounds.width,
            height: bounds.height,
        };
        let origin = (content.x as f32 * scale, content.y as f32 * scale);
        // How much the field changes across one device pixel, from the size the
        // glyph is drawn at, so the edge can fade over exactly one pixel
        // without measuring anything.
        let ramp = morf_text::field_units_per_logical_px(*size as f32) / scale.max(f32::EPSILON);
        let mut push =
            |glyph: RasterGlyph, morph: Option<RasterGlyph>, progress: f32, tint: Color| {
                if glyph.width > 0 && glyph.height > 0 {
                    // A run of text set in its own colour or size: the colour
                    // keeps the node's opacity, the field its own size's ramp.
                    let tint = match glyph.tint {
                        Some([r, g, b, a]) => {
                            let mut run = Color::rgba8(r, g, b, a);
                            run.alpha *= tint.alpha;
                            run
                        }
                        None => tint,
                    };
                    let (ramp, field) = if glyph.font_size > 0.0 {
                        (
                            morf_text::field_units_per_logical_px(glyph.font_size)
                                / scale.max(f32::EPSILON),
                            glyph_field_uniform(*field_style, f64::from(glyph.font_size)),
                        )
                    } else {
                        (ramp, glyph_field_uniform(*field_style, *size))
                    };
                    glyphs.push(PreparedGlyph {
                        glyph,
                        morph,
                        morph_progress: progress,
                        ramp,
                        color: tint,
                        color_overlay: *color_overlay,
                        transform: *transform,
                        command_index,
                        field,
                        outline_color: color_array(field_style.outline_color),
                    });
                }
            };

        if let Some(edit) = edit {
            let selected = edit.selected_text_color.alpha > 0.0 && !edit.selection.is_empty();
            for (glyph, offset) in text_system.rasterize_at(*node, origin, scale) {
                let tint = if selected && edit.selection.contains(&offset) {
                    edit.selected_text_color
                } else {
                    *color
                };
                push(glyph, None, 0.0, tint);
            }
            let (selection, caret) = edit_bands(
                text_system,
                *node,
                edit,
                *horizontal_alignment,
                content,
                *color_overlay,
                *transform,
                command_index,
            );
            under.extend(selection);
            bands.extend(caret);
        } else if morphing {
            text_system.measure_target(
                *node,
                morph_to,
                family,
                *size,
                TextOptions {
                    width: Some(bounds.width),
                    wrap: *wrap,
                    alignment: *horizontal_alignment,
                    elide: *elide,
                    font_weight: *font_weight,
                    font_source: (!font_source.is_empty()).then(|| font_source.clone()),
                    max_lines: *max_lines,
                    style: style.clone(),
                },
            );
            // Paired glyphs come back already measured over one shared box, so
            // both are read through the same quad and the same coordinates —
            // there is nothing left here to reconcile between them.
            // The travel is resolved into a pair of neighbouring frames and a
            // local position between them, so what reaches the shader is always
            // a short step.
            for (glyph, partner, local) in
                text_system.rasterize_pairs(*node, origin, scale, *morph_progress)
            {
                push(glyph, partner, local, *color);
            }
            // Whatever the target has that the source does not is arriving
            // rather than leaving, so it runs the same interpolation backwards:
            // dissolved at zero and whole at one.
            let own = text_system.rasterize(*node, origin, scale, true).len();
            for glyph in text_system
                .rasterize_target(*node, origin, scale, true)
                .into_iter()
                .skip(own)
            {
                push(glyph, None, 1.0 - *morph_progress, *color);
            }
        } else {
            for glyph in text_system.rasterize(*node, origin, scale, true) {
                push(glyph, None, 0.0, *color);
            }
        }
        // The lines under and through runs that ask for them, a link's
        // among them, each in its run's colour.
        // (A text input's runs -- its highlights -- scroll with its content.)
        if style.rich.is_some() {
            let origin = if edit.is_some() {
                content
            } else {
                Geometry {
                    x: bounds.x,
                    y: bounds.y + vertical_offset,
                    width: bounds.width,
                    height: bounds.height,
                }
            };
            for span in text_system.span_bands(*node) {
                let run_color = span.tint.map(|[r, g, b, a]| {
                    let mut run = Color::rgba8(r, g, b, a);
                    run.alpha *= color.alpha;
                    run
                });
                bands.extend(decoration_bands(
                    vec![span.band],
                    &TextDecoration {
                        line: match span.line {
                            morf_text::SpanLine::Under => DecorationLine::Under,
                            morf_text::SpanLine::Through => DecorationLine::Through,
                        },
                        thickness: None,
                        offset: 0.0,
                        color: run_color,
                    },
                    *size,
                    origin,
                    *color,
                    *color_overlay,
                    *transform,
                    command_index,
                    scale,
                ));
            }
        }
        if let Some(decoration) = decoration {
            bands.extend(decoration_bands(
                text_system.line_bands(*node),
                decoration,
                *size,
                Geometry {
                    x: bounds.x,
                    y: bounds.y + vertical_offset,
                    width: bounds.width,
                    height: bounds.height,
                },
                *color,
                *color_overlay,
                *transform,
                command_index,
                scale,
            ));
        }
    }
    if glyphs.is_empty() && bands.is_empty() && under.is_empty() {
        return Ok(None);
    }
    mask_atlas.prepare(queue, &glyphs)?;
    color_atlas.prepare(queue, &glyphs)?;
    let mut instances = Vec::with_capacity(glyphs.len());
    let mut plain = Vec::with_capacity(glyphs.len());
    let mut command_spans: Vec<Vec<GlyphSpan>> =
        (0..list.commands.len()).map(|_| Vec::new()).collect();
    for band in under {
        push_band(
            &mut instances,
            &mut command_spans,
            band,
            scale,
            (target_width, target_height),
        );
        plain.push(false);
    }
    for prepared in glyphs {
        let glyph = prepared.glyph;
        let key = GlyphKey::from_glyph(&glyph);
        let color_glyph = glyph.content == RasterContent::Color;
        let atlas = if color_glyph {
            &*color_atlas
        } else {
            &*mask_atlas
        };
        let entry = atlas.entries.get(&key).ok_or_else(|| {
            GpuError("prepared glyph is missing from the persistent atlas".to_owned())
        })?;
        let tint = match glyph.content {
            RasterContent::Mask | RasterContent::Field => color_array(prepared.color),
            RasterContent::Color => [1.0, 1.0, 1.0, prepared.color.alpha],
        };
        let (origin, axes) = transformed_quad(
            prepared.transform,
            Geometry {
                x: f64::from(glyph.x) / f64::from(scale),
                y: f64::from(glyph.y) / f64::from(scale),
                // The quad, not the bitmap: a distance field is measured once
                // and drawn at any size, so these are the only two of the four
                // that follow the font size rather than the atlas.
                width: f64::from(glyph.draw_width) / f64::from(scale),
                height: f64::from(glyph.draw_height) / f64::from(scale),
            },
            f64::from(scale),
            (target_width, target_height),
        );
        // Where the partner sits in the atlas, or nothing — which the shader
        // reads as empty space so an unpaired letter dissolves.
        let morph_uv = prepared
            .morph
            .as_ref()
            .and_then(|partner| {
                let entry = atlas.entries.get(&GlyphKey::from_glyph(partner))?;
                Some([
                    entry.x as f32 / GLYPH_ATLAS_SIZE as f32,
                    entry.y as f32 / GLYPH_ATLAS_SIZE as f32,
                    partner.width as f32 / GLYPH_ATLAS_SIZE as f32,
                    partner.height as f32 / GLYPH_ATLAS_SIZE as f32,
                ])
            })
            .unwrap_or_default();
        let instance = instances.len() as u32;
        let spans = &mut command_spans[prepared.command_index];
        if let Some(span) = spans.last_mut()
            && span.color == color_glyph
            && span.range.end == instance
        {
            span.range.end = instance + 1;
        } else {
            spans.push(GlyphSpan {
                range: instance..instance + 1,
                color: color_glyph,
                lcd: false,
            });
        }
        plain.push(
            glyph.content == RasterContent::Field
                && super::lcd_spans::only_moves(prepared.transform)
                && prepared.morph.is_none()
                && prepared.morph_progress == 0.0
                && (prepared.field[2] <= 0.0 || prepared.outline_color[3] <= 0.0),
        );
        instances.push(GlyphInstance {
            origin,
            axes,
            uv: [
                entry.x as f32 / GLYPH_ATLAS_SIZE as f32,
                entry.y as f32 / GLYPH_ATLAS_SIZE as f32,
                glyph.width as f32 / GLYPH_ATLAS_SIZE as f32,
                glyph.height as f32 / GLYPH_ATLAS_SIZE as f32,
            ],
            color: tint,
            color_overlay: color_array(prepared.color_overlay),
            mode: [
                0.0,
                0.0,
                if color_glyph { 0.0 } else { 1.0 },
                f32::from(glyph.content == RasterContent::Field),
            ],
            field: [
                prepared.field[0],
                prepared.field[1],
                prepared.field[2],
                prepared.morph_progress,
            ],
            outline_color: prepared.outline_color,
            morph_uv,
            ramp: prepared.ramp,
            ..GlyphInstance::default()
        });
    }
    // A decoration is a solid quad through the glyph pipeline, after the
    // letters of every command so it lies over them, clipped and masked the
    // same way.
    for band in bands {
        push_band(
            &mut instances,
            &mut command_spans,
            band,
            scale,
            (target_width, target_height),
        );
        plain.push(false);
    }
    Ok(Some(GlyphBatch {
        instances,
        command_spans,
        plain,
    }))
}
