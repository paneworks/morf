//! Text set in runs: each run's attributes for the shaper, and what the
//! shaped runs say back — the lines under and through them, and where the
//! links landed.
//!
//! A run reaches cosmic-text as its own attributes with its index (plus one)
//! as metadata, so every laid-out glyph knows which run it came from.

use std::ops::Range;

use cosmic_text::{Align, Attrs, Buffer, FontSystem, Shaping, Style, Weight};
use morf_layout::{TextOptions, TextStyle};
use morf_scene::{NodeHandle, RichSpan, RichText};

use crate::style::{LineBand, face_band, shaping_weight, text_attrs, text_metrics, word_shifts};
use crate::{BufferKey, ResolvedFamily, TextSystem, resolve_family};

pub(crate) fn set_text(
    buffer: &mut Buffer,
    fonts: &FontSystem,
    displayed: &str,
    family: &str,
    size: f32,
    options: &TextOptions,
    align: Option<Align>,
) {
    let family = resolve_family(fonts, family);
    let base = text_attrs(&family, shaping_weight(options), size, &options.style);
    let Some(rich) = &options.style.rich else {
        buffer.set_text(displayed, &base, Shaping::Advanced, align);
        return;
    };
    let families: Vec<_> = rich
        .spans
        .iter()
        .map(|span| {
            span.family
                .as_deref()
                .map(|name| resolve_family(fonts, name))
        })
        .collect();
    let segments = segments(rich, displayed);
    let runs = segments.iter().map(|(range, index)| {
        let attrs = span_attrs(
            &base,
            &rich.spans[*index],
            *index,
            families[*index].as_ref(),
            size,
            &options.style,
        );
        (&displayed[range.clone()], attrs)
    });
    buffer.set_rich_text(runs, &base, Shaping::Advanced, align);
}

/// Which line a run's band is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpanLine {
    Under,
    Through,
}

/// A line under or through one run on one laid-out line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpanBand {
    /// Where it runs and the face's metrics there, from the text's origin.
    pub band: LineBand,
    pub line: SpanLine,
    /// The run's colour; `None` is the node's.
    pub tint: Option<[u8; 4]>,
}

/// Where a link run was laid out, one per line it covers, in logical units
/// from the text's own origin.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkRect {
    pub href: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Which bytes of `displayed` each run covers, when `displayed` is the rich
/// text or an elided form of it (a prefix and a suffix of it kept, something
/// such as an ellipsis between).
pub(crate) fn segments(rich: &RichText, displayed: &str) -> Vec<(Range<usize>, usize)> {
    let text = rich.text.as_str();
    if displayed == text {
        return rich
            .spans
            .iter()
            .enumerate()
            .map(|(index, span)| (span.range.clone(), index))
            .collect();
    }
    let prefix = text
        .char_indices()
        .zip(displayed.chars())
        .take_while(|((_, a), b)| a == b)
        .last()
        .map_or(0, |((at, a), _)| at + a.len_utf8());
    let suffix = text[prefix..]
        .chars()
        .rev()
        .zip(displayed[prefix.min(displayed.len())..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    let tail_start = text.len() - suffix;
    let shift = displayed.len() as isize - text.len() as isize;
    let mut out = Vec::new();
    for (index, span) in rich.spans.iter().enumerate() {
        let head = span.range.start..span.range.end.min(prefix);
        if !head.is_empty() {
            out.push((head, index));
        }
    }
    // What was put between: in the style of the run it replaced the start of.
    let middle = prefix..displayed.len() - suffix;
    if !middle.is_empty() {
        let index = rich
            .spans
            .iter()
            .position(|span| {
                span.range
                    .contains(&prefix.min(text.len().saturating_sub(1)))
            })
            .unwrap_or(0);
        out.push((middle, index));
    }
    for (index, span) in rich.spans.iter().enumerate() {
        let tail = span.range.start.max(tail_start)..span.range.end;
        if !tail.is_empty() {
            let start = (tail.start as isize + shift) as usize;
            let end = (tail.end as isize + shift) as usize;
            out.push((start..end, index));
        }
    }
    out
}

/// A run's attributes: the node's, with what the run changes.
pub(crate) fn span_attrs<'a>(
    base: &Attrs<'a>,
    span: &RichSpan,
    index: usize,
    family: Option<&'a ResolvedFamily>,
    size: f32,
    style: &TextStyle,
) -> Attrs<'a> {
    let mut attrs = base.clone().metadata(index + 1);
    if let Some(family) = family {
        attrs = attrs.family(family.family());
    }
    if let Some(weight) = span.weight {
        attrs = attrs.weight(Weight(weight));
    }
    match span.italic {
        Some(true) => attrs = attrs.style(Style::Italic),
        Some(false) => attrs = attrs.style(Style::Normal),
        None => {}
    }
    if let Some(span_size) = span.size {
        let span_size = span_size as f32;
        attrs = attrs
            .metrics(text_metrics(span_size, style))
            .letter_spacing(style.letter_spacing as f32 / span_size.max(1.0))
            // Optically sized at its own size.
            .font_variations(crate::variations::font_variations(
                &style.variation_axes(span_size),
            ));
    } else {
        let _ = size;
    }
    let color = span.color.or(if span.link.is_some() {
        style.link_color
    } else {
        None
    });
    if let Some(color) = color {
        let [r, g, b, a] = [color.red, color.green, color.blue, color.alpha]
            .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
        attrs = attrs.color(cosmic_text::Color::rgba(r, g, b, a));
    }
    attrs
}

fn tint(glyph: &cosmic_text::LayoutGlyph) -> Option<[u8; 4]> {
    glyph
        .color_opt
        .map(|color| [color.r(), color.g(), color.b(), color.a()])
}

impl TextSystem {
    /// The underlines and strike-throughs of a node's runs, line by line.
    pub fn span_bands(&mut self, node: NodeHandle) -> Vec<SpanBand> {
        let Self { buffers, fonts, .. } = self;
        let Some(cached) = buffers.get(&BufferKey::own(node)) else {
            return Vec::new();
        };
        let Some(rich) = &cached.rich else {
            return Vec::new();
        };
        let mut bands = Vec::new();
        for run in cached.buffer.layout_runs() {
            let (shifts, back) = word_shifts(&run, cached.word_spacing, cached.alignment);
            let mut index = 0;
            while index < run.glyphs.len() {
                let metadata = run.glyphs[index].metadata;
                let mut end = index + 1;
                while end < run.glyphs.len() && run.glyphs[end].metadata == metadata {
                    end += 1;
                }
                let span = metadata.checked_sub(1).and_then(|at| rich.spans.get(at));
                if let Some(span) = span.filter(|span| span.underline || span.strike) {
                    let glyphs = &run.glyphs[index..end];
                    let left = glyphs
                        .iter()
                        .zip(&shifts[index..end])
                        .map(|(glyph, shift)| glyph.x + shift - back)
                        .fold(f32::MAX, f32::min);
                    let right = glyphs
                        .iter()
                        .zip(&shifts[index..end])
                        .map(|(glyph, shift)| glyph.x + glyph.w + shift - back)
                        .fold(f32::MIN, f32::max);
                    let band =
                        face_band(fonts, &glyphs[0], left, (right - left).max(0.0), run.line_y);
                    for (wanted, line) in [
                        (span.underline, SpanLine::Under),
                        (span.strike, SpanLine::Through),
                    ] {
                        if wanted {
                            bands.push(SpanBand {
                                band,
                                line,
                                tint: tint(&glyphs[0]),
                            });
                        }
                    }
                }
                index = end;
            }
        }
        bands
    }

    /// How tall a node's shaped text is, as measuring it said.
    pub fn shaped_height(&self, node: NodeHandle) -> f32 {
        self.buffers
            .get(&BufferKey::own(node))
            .map_or(0.0, |cached| {
                cached
                    .buffer
                    .layout_runs()
                    .map(|run| run.line_top + run.line_height)
                    .fold(0.0, f32::max)
            })
    }

    /// Where each link run of a node was laid out.
    pub fn link_rects(&self, node: NodeHandle) -> Vec<LinkRect> {
        let Some(cached) = self.buffers.get(&BufferKey::own(node)) else {
            return Vec::new();
        };
        let Some(rich) = cached.rich.as_ref().filter(|rich| rich.has_links()) else {
            return Vec::new();
        };
        let mut rects = Vec::new();
        for run in cached.buffer.layout_runs() {
            let (shifts, back) = word_shifts(&run, cached.word_spacing, cached.alignment);
            let mut index = 0;
            while index < run.glyphs.len() {
                let metadata = run.glyphs[index].metadata;
                let mut end = index + 1;
                while end < run.glyphs.len() && run.glyphs[end].metadata == metadata {
                    end += 1;
                }
                if let Some(href) = metadata
                    .checked_sub(1)
                    .and_then(|at| rich.spans.get(at))
                    .and_then(|span| span.link.clone())
                {
                    let glyphs = &run.glyphs[index..end];
                    let left = glyphs
                        .iter()
                        .zip(&shifts[index..end])
                        .map(|(glyph, shift)| glyph.x + shift - back)
                        .fold(f32::MAX, f32::min);
                    let right = glyphs
                        .iter()
                        .zip(&shifts[index..end])
                        .map(|(glyph, shift)| glyph.x + glyph.w + shift - back)
                        .fold(f32::MIN, f32::max);
                    rects.push(LinkRect {
                        href,
                        x: left,
                        y: run.line_top,
                        width: (right - left).max(0.0),
                        height: run.line_height,
                    });
                }
                index = end;
            }
        }
        rects
    }
}
