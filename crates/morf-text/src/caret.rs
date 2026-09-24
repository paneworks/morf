//! Where a caret stands in shaped text, and which offset a point lands on.
//!
//! Read off a node's shaped buffer once and kept as plain numbers: a stop at
//! every grapheme boundary of every laid-out line, with the x it sits at. The
//! caret, the selection's rectangles, a click, and up and down between lines
//! are then all lookups in that table, which is what lets the runtime answer
//! them between frames without a font in sight, and lets a test answer them
//! with no fonts installed at all.

use std::collections::BTreeMap;

use cosmic_text::Buffer;
use morf_layout::TextAlignment;
use morf_scene::NodeHandle;
use unicode_segmentation::UnicodeSegmentation;

use crate::style::word_shifts;
use crate::{BufferKey, TextSystem};

/// The caret at one offset: a vertical bar from `y` down `height`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CaretRect {
    pub x: f32,
    pub y: f32,
    pub height: f32,
}

/// One rectangle of a selection.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpanRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// One laid-out line and the caret stops along it.
#[derive(Clone, Debug, PartialEq)]
pub struct CaretLine {
    /// Top of the line box, from the text's origin.
    pub top: f32,
    /// Height of the line box.
    pub height: f32,
    /// First byte on the line.
    pub start: usize,
    /// Byte after the last one on the line, before any line break.
    pub end: usize,
    /// Whether the line ends at a break or the end of the text, rather than
    /// being wrapped onto the next.
    pub hard_end: bool,
    /// `(offset, x)` at every grapheme boundary, in offset order.
    stops: Vec<(usize, f32)>,
}

impl CaretLine {
    fn x_of(&self, offset: usize) -> f32 {
        match self.stops.binary_search_by_key(&offset, |(byte, _)| *byte) {
            Ok(index) => self.stops[index].1,
            Err(0) => self.stops.first().map_or(0.0, |(_, x)| *x),
            Err(index) => self.stops[index - 1].1,
        }
    }

    /// The stop nearest `x`. A wrapped line's last stop is the next line's
    /// first, so it is not offered: a click at the end of a wrapped line
    /// lands before its last letter rather than on the line below.
    fn nearest(&self, x: f32) -> usize {
        let usable = if self.hard_end || self.stops.len() < 2 {
            &self.stops[..]
        } else {
            &self.stops[..self.stops.len() - 1]
        };
        usable
            .iter()
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map_or(self.start, |(byte, _)| *byte)
    }
}

/// Every caret stop of a shaped text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaretMap {
    lines: Vec<CaretLine>,
}

impl CaretMap {
    /// A map for text set in a uniform advance: every grapheme `advance`
    /// wide, every line `line_height` tall, nothing wrapped.
    ///
    /// What a field falls back to before it has ever been shaped — and what
    /// a test uses to reason about carets with no fonts to hand.
    pub fn uniform(text: &str, advance: f32, line_height: f32) -> Self {
        let mut lines = Vec::new();
        let mut base = 0;
        for (index, line) in text.split('\n').enumerate() {
            let mut stops = vec![(base, 0.0)];
            for (count, (start, grapheme)) in line.grapheme_indices(true).enumerate() {
                stops.push((base + start + grapheme.len(), (count + 1) as f32 * advance));
            }
            lines.push(CaretLine {
                top: index as f32 * line_height,
                height: line_height,
                start: base,
                end: base + line.len(),
                hard_end: true,
                stops,
            });
            base += line.len() + 1;
        }
        Self { lines }
    }

    /// The laid-out lines, top to bottom.
    pub fn lines(&self) -> &[CaretLine] {
        &self.lines
    }

    /// Which line an offset's caret is drawn on.
    ///
    /// The offset where a wrapped line meets the next is on both; it is drawn
    /// at the start of the second, where typing there would appear.
    pub fn line_of(&self, offset: usize) -> usize {
        let mut fallback = None;
        for (index, line) in self.lines.iter().enumerate() {
            if offset < line.start {
                // Inside a line break, which belongs to the line before.
                return fallback.unwrap_or(index);
            }
            if offset <= line.end {
                if offset == line.end && !line.hard_end && index + 1 < self.lines.len() {
                    fallback = Some(index);
                    continue;
                }
                return index;
            }
            fallback = Some(index);
        }
        fallback.unwrap_or(0)
    }

    /// The caret at an offset.
    pub fn caret(&self, offset: usize) -> CaretRect {
        let Some(line) = self.lines.get(self.line_of(offset)) else {
            return CaretRect::default();
        };
        CaretRect {
            x: line.x_of(offset),
            y: line.top,
            height: line.height,
        }
    }

    /// The offset nearest a point, the point clamped into the text.
    pub fn index_at(&self, x: f32, y: f32) -> usize {
        let index = self
            .lines
            .iter()
            .position(|line| y < line.top + line.height)
            .unwrap_or(self.lines.len().saturating_sub(1));
        self.index_on_line(index, x)
    }

    /// The offset on one line nearest an x.
    pub fn index_on_line(&self, line: usize, x: f32) -> usize {
        self.lines.get(line).map_or(0, |line| line.nearest(x))
    }

    /// The offset `lines` lines above (negative) or below an offset, at `x`;
    /// nothing when that runs off the text.
    pub fn vertical(&self, offset: usize, lines: isize, x: f32) -> Option<usize> {
        let target = self.line_of(offset) as isize + lines;
        (0..self.lines.len() as isize)
            .contains(&target)
            .then(|| self.index_on_line(target as usize, x))
    }

    /// Where Home and End go on the line an offset is drawn on.
    ///
    /// A wrapped line's end is the next line's start, so End stops before
    /// the last grapheme — the space it wrapped at, as a rule — to stay on
    /// the line it was pressed on.
    pub fn line_bounds(&self, offset: usize) -> (usize, usize) {
        let Some(line) = self.lines.get(self.line_of(offset)) else {
            return (0, 0);
        };
        let end = if line.hard_end || line.stops.len() < 2 {
            line.end
        } else {
            line.stops[line.stops.len() - 2].0
        };
        (line.start, end)
    }

    /// The rectangles covering a selection, one per line it touches.
    ///
    /// A selection that runs on past a line's break carries a sliver past the
    /// line's end, so a selected empty line and a selected newline are seen.
    pub fn selection(&self, start: usize, end: usize) -> Vec<SpanRect> {
        let (start, end) = (start.min(end), start.max(end));
        if start == end {
            return Vec::new();
        }
        let mut rects = Vec::new();
        let last = self.lines.len().saturating_sub(1);
        for (index, line) in self.lines.iter().enumerate() {
            if end < line.start || start > line.end {
                continue;
            }
            let from = start.max(line.start);
            let to = end.min(line.end);
            let (mut left, mut right) = (line.x_of(from), line.x_of(to));
            if left > right {
                std::mem::swap(&mut left, &mut right);
            }
            if end > line.end && line.hard_end && index < last {
                right += line.height * 0.3;
            }
            if right > left {
                rects.push(SpanRect {
                    x: left,
                    y: line.top,
                    width: right - left,
                    height: line.height,
                });
            }
        }
        rects
    }

    /// The extent of the laid-out text: its left and right edges, and how
    /// tall its lines are together.
    pub fn extent(&self) -> (f32, f32, f32) {
        let mut left = f32::MAX;
        let mut right = f32::MIN;
        for (_, x) in self.lines.iter().flat_map(|line| line.stops.iter()) {
            left = left.min(*x);
            right = right.max(*x);
        }
        let bottom = self.lines.last().map_or(0.0, |line| line.top + line.height);
        if left > right {
            (0.0, 0.0, bottom)
        } else {
            (left, right, bottom)
        }
    }
}

/// Reads the stops out of a shaped buffer.
pub(crate) fn caret_map(buffer: &Buffer, word_spacing: f32, alignment: TextAlignment) -> CaretMap {
    // A glyph's offsets are into its own line of the buffer, which split the
    // text at its breaks; these are where those lines start in the whole.
    let mut bases = Vec::with_capacity(buffer.lines.len());
    let mut base = 0;
    for line in &buffer.lines {
        bases.push(base);
        base += line.text().len() + line.ending().as_str().len();
    }
    let width = buffer.size().0;
    let mut lines: Vec<CaretLine> = Vec::new();
    let mut previous_line = usize::MAX;
    for run in buffer.layout_runs() {
        let base = bases.get(run.line_i).copied().unwrap_or(0);
        if let Some(last) = lines.last_mut()
            && previous_line != run.line_i
        {
            last.hard_end = true;
        }
        previous_line = run.line_i;
        let mut stops = BTreeMap::new();
        let (shifts, back) = word_shifts(&run, word_spacing, alignment);
        for (glyph, shift) in run.glyphs.iter().zip(shifts) {
            let left = glyph.x + shift - back;
            let right = left + glyph.w;
            let (from, to) = if glyph.level.is_rtl() {
                (right, left)
            } else {
                (left, right)
            };
            let cluster = run.text.get(glyph.start..glyph.end).unwrap_or_default();
            let graphemes = cluster.grapheme_indices(true).collect::<Vec<_>>();
            let count = graphemes.len().max(1) as f32;
            // A glyph's start is where the caret before it stands, even when
            // the glyph before ended somewhere else — word spacing moves a
            // word along, and the caret after a space goes with the word.
            stops.insert(base + glyph.start, from);
            for (index, (offset, grapheme)) in graphemes.iter().enumerate() {
                let x = from + (to - from) * (index + 1) as f32 / count;
                stops
                    .entry(base + glyph.start + offset + grapheme.len())
                    .or_insert(x);
            }
        }
        let (start, end) = match (stops.keys().next(), stops.keys().next_back()) {
            (Some(start), Some(end)) => (*start, *end),
            _ => {
                // An empty line: the caret stands where its alignment puts
                // nothing.
                let x = match (alignment, width) {
                    (TextAlignment::Center, Some(width)) => width / 2.0,
                    (TextAlignment::Right, Some(width)) => width,
                    _ => 0.0,
                };
                stops.insert(base, x);
                (base, base)
            }
        };
        lines.push(CaretLine {
            top: run.line_top,
            height: run.line_height,
            start,
            end,
            hard_end: false,
            stops: stops.into_iter().collect(),
        });
    }
    if let Some(last) = lines.last_mut() {
        last.hard_end = true;
    }
    CaretMap { lines }
}

impl TextSystem {
    /// The caret stops of a node's shaped text, as it was last measured.
    pub fn caret_map(&self, node: NodeHandle) -> Option<CaretMap> {
        let cached = self.buffers.get(&BufferKey::own(node))?;
        Some(caret_map(
            &cached.buffer,
            cached.word_spacing,
            cached.alignment,
        ))
    }
}
