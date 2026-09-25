//! How a style reaches the shaper, and what the shaper's lines say back.
//!
//! Letter spacing, slant and width go into the attributes cosmic-text shapes
//! with; line height goes into its metrics. Word spacing it has no word for,
//! so that one is added after shaping: every glyph moves right by the spaces
//! before it on its line, and a centred or right-aligned line moves back by
//! what the whole line gained.

use cosmic_text::{Attrs, Buffer, LayoutRun, Metrics, PhysicalGlyph, Stretch, Style, Weight};
use morf_layout::{FontStretch, FontStyle, TextAlignment, TextStyle};
use morf_scene::NodeHandle;

use crate::{BufferKey, CachedBuffer, ResolvedFamily, TextSystem};

pub(crate) fn text_attrs<'a>(
    family: &'a ResolvedFamily,
    weight: u16,
    size: f32,
    style: &TextStyle,
) -> Attrs<'a> {
    let slant = match style.font_style {
        FontStyle::Normal => Style::Normal,
        FontStyle::Italic => Style::Italic,
        FontStyle::Oblique => Style::Oblique,
    };
    let stretch = match style.font_stretch {
        FontStretch::UltraCondensed => Stretch::UltraCondensed,
        FontStretch::ExtraCondensed => Stretch::ExtraCondensed,
        FontStretch::Condensed => Stretch::Condensed,
        FontStretch::SemiCondensed => Stretch::SemiCondensed,
        FontStretch::Normal => Stretch::Normal,
        FontStretch::SemiExpanded => Stretch::SemiExpanded,
        FontStretch::Expanded => Stretch::Expanded,
        FontStretch::ExtraExpanded => Stretch::ExtraExpanded,
        FontStretch::UltraExpanded => Stretch::UltraExpanded,
    };
    // Tracking is asked for in em, so pixels are divided out by the size.
    Attrs::new()
        .family(family.family())
        .weight(Weight(weight))
        .style(slant)
        .stretch(stretch)
        .letter_spacing(style.letter_spacing as f32 / size.max(1.0))
        .font_variations(crate::variations::font_variations(
            &style.variation_axes(size),
        ))
}

/// The weight text is shaped at: a `wght` among the axes wins over
/// `font_weight`.
pub(crate) fn shaping_weight(options: &morf_layout::TextOptions) -> u16 {
    crate::normalize_font_weight(
        options
            .style
            .axis(b"wght")
            .map_or(options.font_weight, f64::from),
    )
}

pub(crate) fn text_metrics(size: f32, style: &TextStyle) -> Metrics {
    Metrics::new(size, style.line_height.pixels(f64::from(size)) as f32)
}

fn is_space(run: &LayoutRun<'_>, glyph: &cosmic_text::LayoutGlyph) -> bool {
    run.text
        .get(glyph.start..glyph.end)
        .is_some_and(|cluster| !cluster.is_empty() && cluster.chars().all(char::is_whitespace))
}

/// How far each glyph of a run moves for word spacing, and how far the line
/// as a whole moves back for its alignment.
pub(crate) fn word_shifts(
    run: &LayoutRun<'_>,
    word_spacing: f32,
    alignment: TextAlignment,
) -> (Vec<f32>, f32) {
    if word_spacing == 0.0 {
        return (vec![0.0; run.glyphs.len()], 0.0);
    }
    let mut spaces = 0.0;
    let shifts = run
        .glyphs
        .iter()
        .map(|glyph| {
            let shift = spaces * word_spacing;
            if is_space(run, glyph) {
                spaces += 1.0;
            }
            shift
        })
        .collect();
    let gained = spaces * word_spacing;
    let back = match alignment {
        TextAlignment::Left | TextAlignment::Justified => 0.0,
        TextAlignment::Center => gained / 2.0,
        TextAlignment::Right => gained,
    };
    (shifts, back)
}

/// What a run of a buffer gained in width from word spacing.
pub(crate) fn run_gain(run: &LayoutRun<'_>, word_spacing: f32) -> f32 {
    if word_spacing == 0.0 {
        return 0.0;
    }
    run.glyphs
        .iter()
        .filter(|glyph| is_space(run, glyph))
        .count() as f32
        * word_spacing
}

/// Every glyph of a shaped buffer at its physical place, word spacing applied.
pub(crate) fn physical_glyphs(
    cached: &CachedBuffer,
    origin: (f32, f32),
    scale: f32,
) -> Vec<PhysicalGlyph> {
    let mut glyphs = Vec::new();
    for run in cached.buffer.layout_runs() {
        let (shifts, back) = word_shifts(&run, cached.word_spacing, cached.alignment);
        for (glyph, shift) in run.glyphs.iter().zip(shifts) {
            glyphs.push(glyph.physical(
                (
                    origin.0 + (shift - back) * scale,
                    origin.1 + run.line_y * scale,
                ),
                scale,
            ));
        }
    }
    glyphs
}

/// Every glyph of a shaped buffer at its physical place, with its run's
/// colour and size when the text was set in runs.
pub(crate) fn physical_glyphs_styled(
    cached: &CachedBuffer,
    origin: (f32, f32),
    scale: f32,
) -> Vec<(PhysicalGlyph, Option<[u8; 4]>, f32)> {
    let styled = cached.rich.is_some();
    let mut glyphs = Vec::new();
    for run in cached.buffer.layout_runs() {
        let (shifts, back) = word_shifts(&run, cached.word_spacing, cached.alignment);
        for (glyph, shift) in run.glyphs.iter().zip(shifts) {
            let physical = glyph.physical(
                (
                    origin.0 + (shift - back) * scale,
                    origin.1 + run.line_y * scale,
                ),
                scale,
            );
            let tint = glyph
                .color_opt
                .filter(|_| styled)
                .map(|color| [color.r(), color.g(), color.b(), color.a()]);
            glyphs.push((physical, tint, if styled { glyph.font_size } else { 0.0 }));
        }
    }
    glyphs
}

/// Every glyph of a shaped buffer at its physical place, with the offset in
/// the whole text its cluster starts at — which a selection needs to know
/// which glyphs it covers.
pub(crate) fn physical_glyphs_at(
    cached: &CachedBuffer,
    origin: (f32, f32),
    scale: f32,
) -> Vec<(PhysicalGlyph, usize)> {
    let mut bases = Vec::with_capacity(cached.buffer.lines.len());
    let mut base = 0;
    for line in &cached.buffer.lines {
        bases.push(base);
        base += line.text().len() + line.ending().as_str().len();
    }
    let mut glyphs = Vec::new();
    for run in cached.buffer.layout_runs() {
        let base = bases.get(run.line_i).copied().unwrap_or(0);
        let (shifts, back) = word_shifts(&run, cached.word_spacing, cached.alignment);
        for (glyph, shift) in run.glyphs.iter().zip(shifts) {
            glyphs.push((
                glyph.physical(
                    (
                        origin.0 + (shift - back) * scale,
                        origin.1 + run.line_y * scale,
                    ),
                    scale,
                ),
                base + glyph.start,
            ));
        }
    }
    glyphs
}

/// One laid-out line, as a decoration needs it: where it runs and where the
/// face puts a line under, over or through it. Logical units from the text's
/// own origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineBand {
    pub x: f32,
    pub width: f32,
    pub baseline: f32,
    /// From the baseline up to the top of the face's box.
    pub ascent: f32,
    /// From the baseline down to the top of an underline; positive below.
    pub underline_offset: f32,
    /// From the baseline up to the top of a strikeout.
    pub strikeout_offset: f32,
    /// The face's recommended thickness for either.
    pub stroke_size: f32,
}

impl TextSystem {
    /// The lines of a node's shaped text, for drawing decorations along.
    pub fn line_bands(&mut self, node: NodeHandle) -> Vec<LineBand> {
        let Self { buffers, fonts, .. } = self;
        let Some(cached) = buffers.get(&BufferKey::own(node)) else {
            return Vec::new();
        };
        line_bands_of(&cached.buffer, cached.word_spacing, cached.alignment, fonts)
    }
}

fn line_bands_of(
    buffer: &Buffer,
    word_spacing: f32,
    alignment: TextAlignment,
    fonts: &mut cosmic_text::FontSystem,
) -> Vec<LineBand> {
    let mut bands = Vec::new();
    for run in buffer.layout_runs() {
        let Some(first) = run.glyphs.first() else {
            continue;
        };
        let (shifts, back) = word_shifts(&run, word_spacing, alignment);
        let left = run
            .glyphs
            .iter()
            .zip(&shifts)
            .map(|(glyph, shift)| glyph.x + shift - back)
            .fold(f32::MAX, f32::min);
        let right = run
            .glyphs
            .iter()
            .zip(&shifts)
            .map(|(glyph, shift)| glyph.x + glyph.w + shift - back)
            .fold(f32::MIN, f32::max);
        bands.push(face_band(
            fonts,
            first,
            left,
            (right - left).max(0.0),
            run.line_y,
        ));
    }
    bands
}

/// A band along `x..x + width` at `baseline`, with the lines where `glyph`'s
/// face puts them.
pub(crate) fn face_band(
    fonts: &mut cosmic_text::FontSystem,
    glyph: &cosmic_text::LayoutGlyph,
    x: f32,
    width: f32,
    baseline: f32,
) -> LineBand {
    let size = glyph.font_size;
    // The face's own recommendation, scaled to the size; a face that
    // cannot be found falls back to proportions that read right on most.
    let metrics = fonts
        .get_font(glyph.font_id, glyph.font_weight)
        .map(|font| font.as_swash().metrics(&[]).scale(size));
    let (ascent, underline_offset, strikeout_offset, stroke_size) = match metrics {
        Some(metrics) => (
            metrics.ascent,
            -metrics.underline_offset,
            metrics.strikeout_offset,
            metrics.stroke_size,
        ),
        None => (size * 0.8, size * 0.1, size * 0.3, size / 14.0),
    };
    LineBand {
        x,
        width,
        baseline,
        ascent,
        underline_offset: if underline_offset > 0.0 {
            underline_offset
        } else {
            size * 0.1
        },
        strikeout_offset: if strikeout_offset > 0.0 {
            strikeout_offset
        } else {
            size * 0.3
        },
        stroke_size: if stroke_size > 0.0 {
            stroke_size
        } else {
            size / 14.0
        },
    }
}
