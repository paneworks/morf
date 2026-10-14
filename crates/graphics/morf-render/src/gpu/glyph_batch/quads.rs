//! The quads a frame's text is drawn as: one instance per band and glyph,
//! the bands beneath first, then the glyphs, then the bands over them.
//!
//! A quad is a function of its item, the scale, the target and where its
//! glyph sits in the atlas. So a command copied from the last list is drawn
//! as the quads it was drawn as then, for as long as the atlases have moved
//! nothing (`GlyphAtlas::generation`) and the target kept its size.

use morf_layout::Geometry;
use morf_text::RasterContent;
use std::collections::HashMap;

use super::PreparedBand;
use super::bands::band_instance;
use crate::effects::color_array;
use crate::gpu::backend_types::GpuError;
use crate::gpu::glyphs::*;
use crate::{CopiedRun, RunCursor};

/// Where laid-out glyphs go in [`super::Laid`] order, and in the runs a
/// copier records.
pub(crate) const GLYPHS: usize = 0;
pub(crate) const UNDER: usize = 1;
pub(crate) const OVER: usize = 2;

/// An atlas as a quad reads it: where each glyph sits, in a texture how
/// tall.
#[derive(Clone, Copy)]
pub(crate) struct AtlasView<'a> {
    pub(crate) entries: &'a HashMap<GlyphKey, GlyphAtlasEntry>,
    pub(crate) height: u32,
}

/// The last frame's quads, and which of this frame's items are its.
#[derive(Clone, Copy)]
pub(crate) struct KeptQuads<'a> {
    /// In batch order: the bands beneath, the glyphs, the bands over.
    pub(crate) instances: &'a [GlyphInstance],
    pub(crate) plain: &'a [bool],
    /// Where each kind of item begins in `instances`, by [`GLYPHS`],
    /// [`UNDER`] and [`OVER`].
    pub(crate) bases: [usize; 3],
    /// Per kind, the items copied: `to` in this frame's, `from` in the kept.
    pub(crate) runs: &'a [Vec<CopiedRun>; 3],
}

/// Every item's quad and span, read from `kept` where it was copied.
pub(crate) fn glyph_quads(
    atlases: (AtlasView<'_>, AtlasView<'_>),
    (glyphs, under, over): (&[PreparedGlyph], &[PreparedBand], &[PreparedBand]),
    kept: Option<KeptQuads<'_>>,
    commands: usize,
    scale: f32,
    target: (u32, u32),
) -> Result<GlyphBatch, GpuError> {
    let total = glyphs.len() + under.len() + over.len();
    let mut batch = GlyphBatch {
        instances: Vec::with_capacity(total),
        command_spans: (0..commands).map(|_| Vec::new()).collect(),
        plain: Vec::with_capacity(total),
        atlases: [0; 2],
    };
    let band = |band: &PreparedBand| Ok((band_instance(band, scale, target), false));
    let section = |kind: usize| kept.map(|kept| (kept, kind));
    push_items(
        &mut batch,
        under,
        section(UNDER),
        |band| (band.command_index, false),
        band,
    )?;
    push_items(
        &mut batch,
        glyphs,
        section(GLYPHS),
        |glyph| {
            let color = glyph.glyph.content == RasterContent::Color;
            (glyph.command_index, color)
        },
        |glyph| glyph_instance(glyph, atlases, scale, target),
    )?;
    push_items(
        &mut batch,
        over,
        section(OVER),
        |band| (band.command_index, false),
        band,
    )?;
    Ok(batch)
}

/// Appends one kind of item: each one's quad, and its place in its
/// command's spans by `span` (its command, whether it is colour).
fn push_items<T>(
    batch: &mut GlyphBatch,
    items: &[T],
    kept: Option<(KeptQuads<'_>, usize)>,
    span: impl Fn(&T) -> (usize, bool),
    make: impl Fn(&T) -> Result<(GlyphInstance, bool), GpuError>,
) -> Result<(), GpuError> {
    let mut cursor = kept.map(|(kept, kind)| (kept, kind, RunCursor::new(&kept.runs[kind])));
    for (index, item) in items.iter().enumerate() {
        let copied = cursor.as_mut().and_then(|(kept, kind, cursor)| {
            let at = kept.bases[*kind] + cursor.source(index)?;
            Some((*kept.instances.get(at)?, *kept.plain.get(at)?))
        });
        let (instance, plain) = match copied {
            Some(copied) => copied,
            None => make(item)?,
        };
        let (command, color) = span(item);
        let at = batch.instances.len() as u32;
        let spans = &mut batch.command_spans[command];
        if let Some(span) = spans.last_mut()
            && span.color == color
            && span.range.end == at
        {
            span.range.end = at + 1;
        } else {
            spans.push(GlyphSpan {
                range: at..at + 1,
                color,
                lcd: false,
            });
        }
        batch.instances.push(instance);
        batch.plain.push(plain);
    }
    Ok(())
}

/// A glyph's quad, and whether subpixel text may draw it.
fn glyph_instance(
    prepared: &PreparedGlyph,
    (mask, color): (AtlasView<'_>, AtlasView<'_>),
    scale: f32,
    target: (u32, u32),
) -> Result<(GlyphInstance, bool), GpuError> {
    let glyph = &prepared.glyph;
    let color_glyph = glyph.content == RasterContent::Color;
    let atlas = if color_glyph { color } else { mask };
    let entry = atlas
        .entries
        .get(&GlyphKey::from_glyph(glyph))
        .ok_or_else(|| {
            GpuError::Other("prepared glyph is missing from the persistent atlas".to_owned())
        })?;
    let atlas_height = atlas.height as f32;
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
        target,
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
                entry.y as f32 / atlas_height,
                partner.width as f32 / GLYPH_ATLAS_SIZE as f32,
                partner.height as f32 / atlas_height,
            ])
        })
        .unwrap_or_default();
    let plain = glyph.content == RasterContent::Field
        && crate::gpu::lcd_spans::only_moves(prepared.transform)
        && prepared.morph.is_none()
        && prepared.morph_progress == 0.0
        && (prepared.field[2] <= 0.0 || prepared.outline_color[3] <= 0.0);
    let instance = GlyphInstance {
        origin,
        axes,
        uv: [
            entry.x as f32 / GLYPH_ATLAS_SIZE as f32,
            entry.y as f32 / atlas_height,
            glyph.width as f32 / GLYPH_ATLAS_SIZE as f32,
            glyph.height as f32 / atlas_height,
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
    };
    Ok((instance, plain))
}
