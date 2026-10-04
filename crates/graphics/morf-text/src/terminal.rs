//! Type for a terminal: one cell per character, on a grid.
//!
//! A terminal is not shaped text. Each character is drawn in its own cell,
//! at its column times the cell's width, whatever the font would have
//! advanced it by — a fallback face with a wider glyph must not push the rest
//! of the row out of line. So a character is shaped once, on its own (which
//! is where font fallback happens), and cached; drawing a cell is a lookup
//! and a placement.
//!
//! The characters that are meant to join their neighbours — box drawing,
//! block elements, braille, powerline separators — are not taken from a font
//! at all. A font's `│` stops short of its line height, so a box drawn from
//! it is a dotted outline wherever the cell is taller than the face; drawn
//! here, from the cell's own size, a line runs edge to edge and meets the
//! next one on the same pixel. That is what makes btop's frames and graphs
//! line up.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use cosmic_text::{
    Attrs, Buffer, CacheKey, LayoutGlyph, Metrics, Shaping, Style, SwashContent, Weight,
};
use morf_scene::TerminalMetrics;

use crate::{RasterContent, RasterGlyph, TextSystem, resolve_family};

mod box_arms;
mod cell_drawing;

use cell_drawing::draw_cell;

/// One character (with anything combining on it) in one face and style.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct CellShapeKey {
    text: Box<str>,
    family: Box<str>,
    size_bits: u32,
    bold: bool,
    italic: bool,
}

/// A rasterized glyph image, before it is put anywhere.
#[derive(Clone)]
pub(crate) struct CellImage {
    hash: u64,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    content: RasterContent,
    data: Rc<Vec<u8>>,
}

/// A lone character in one size and style: its size's bits, bold, italic.
type CharKey = (char, u32, bool, bool);

/// A drawn cell's coverage, a byte a pixel.
type Coverage = Rc<Vec<u8>>;

/// What a terminal's type caches.
#[derive(Default)]
pub(crate) struct TerminalCache {
    metrics: crate::FastMap<(Box<str>, u32), TerminalMetrics>,
    shapes: crate::FastMap<CellShapeKey, Rc<[LayoutGlyph]>>,
    /// The same, for a lone character, found without building a key: a
    /// screen is thousands of these a frame.
    chars: crate::FastMap<Box<str>, crate::FastMap<CharKey, Rc<[LayoutGlyph]>>>,
    images: crate::FastMap<CacheKey, Option<CellImage>>,
    drawn: crate::FastMap<(char, u32, u32), Option<Coverage>>,
}

/// The type a terminal's grid is set in, and the scale it is drawn at.
#[derive(Clone, Copy, Debug)]
pub struct CellFace<'a> {
    pub family: &'a str,
    pub size: f64,
    pub metrics: TerminalMetrics,
    /// Device pixels per logical pixel.
    pub scale: f32,
}

/// A character as a terminal cell holds it.
#[derive(Clone, Copy, Debug)]
pub struct CellText<'a> {
    pub character: char,
    pub combining: Option<&'a str>,
    pub bold: bool,
    pub italic: bool,
}

impl TextSystem {
    /// The cell a terminal in this face and size is laid out on, in whole
    /// logical pixels: as wide as the face's advance, as tall as its ascent
    /// and descent.
    pub fn terminal_metrics(&mut self, family: &str, size: f64) -> TerminalMetrics {
        let size = size.clamp(1.0, 512.0) as f32;
        let key = (Box::<str>::from(family), size.to_bits());
        if let Some(metrics) = self.terminal.metrics.get(&key) {
            return *metrics;
        }
        let glyphs = self.cell_shape(&CellShapeKey {
            text: "M".into(),
            family: family.into(),
            size_bits: size.to_bits(),
            bold: false,
            italic: false,
        });
        let advance = glyphs.first().map_or(size * 0.6, |glyph| glyph.w);
        let face = glyphs.first().and_then(|glyph| {
            self.fonts
                .get_font(glyph.font_id, glyph.font_weight)
                .map(|font| font.as_swash().metrics(&[]).scale(size))
        });
        let (ascent, descent, underline, strikeout, stroke) = match face {
            Some(face) => (
                face.ascent,
                face.descent.abs(),
                -face.underline_offset,
                face.strikeout_offset,
                face.stroke_size,
            ),
            None => (size * 0.8, size * 0.2, size * 0.1, size * 0.3, size / 14.0),
        };
        let height = (ascent + descent).round().max(1.0);
        // Whatever rounding added is shared above and below the face.
        let baseline = (ascent + (height - ascent - descent) / 2.0).round();
        let metrics = TerminalMetrics {
            cell_width: f64::from(advance.round().max(1.0)),
            cell_height: f64::from(height),
            baseline: f64::from(baseline),
            underline_offset: f64::from(if underline > 0.0 {
                underline
            } else {
                size * 0.1
            }),
            strikeout_offset: f64::from(if strikeout > 0.0 {
                strikeout
            } else {
                size * 0.3
            }),
            stroke: f64::from(if stroke > 0.0 { stroke } else { size / 14.0 }).max(1.0),
        };
        self.terminal.metrics.insert(key, metrics);
        metrics
    }

    /// The glyphs of one cell, placed with the cell's top-left corner at
    /// `origin` (physical pixels) on a grid of `face.metrics` at
    /// `face.scale`.
    ///
    /// Box drawing, block elements, braille and powerline separators come
    /// back as one image the size of the cell (two for a wide one); anything
    /// else is the face's glyph (or a fallback face's) on the baseline.
    pub fn terminal_glyphs(
        &mut self,
        cell: CellText<'_>,
        face: &CellFace<'_>,
        origin: (f32, f32),
        out: &mut Vec<RasterGlyph>,
    ) {
        let CellFace {
            family,
            size,
            metrics,
            scale,
        } = *face;
        let width = (metrics.cell_width as f32 * scale).round().max(1.0) as u32;
        let height = (metrics.cell_height as f32 * scale).round().max(1.0) as u32;
        if cell.combining.is_none()
            && let Some(image) = self.drawn_cell(cell.character, width, height, scale)
        {
            let mut hasher = DefaultHasher::new();
            (0x7e41_u32, cell.character, width, height).hash(&mut hasher);
            out.push(RasterGlyph {
                cache_key: hasher.finish(),
                x: origin.0.round(),
                y: origin.1.round(),
                width,
                height,
                draw_width: width as f32,
                draw_height: height as f32,
                content: RasterContent::Mask,
                tint: None,
                font_size: 0.0,
                data: image,
            });
            return;
        }
        let size = size.clamp(1.0, 512.0) as f32;
        let quick = (cell.character, size.to_bits(), cell.bold, cell.italic);
        let known = cell
            .combining
            .is_none()
            .then(|| self.terminal.chars.get(family)?.get(&quick).cloned())
            .flatten();
        let glyphs = match known {
            Some(glyphs) => glyphs,
            None => {
                let mut text = String::new();
                text.push(cell.character);
                if let Some(combining) = cell.combining {
                    text.push_str(combining);
                }
                let glyphs = self.cell_shape(&CellShapeKey {
                    text: text.into(),
                    family: family.into(),
                    size_bits: size.to_bits(),
                    bold: cell.bold,
                    italic: cell.italic,
                });
                if cell.combining.is_none() {
                    self.terminal
                        .chars
                        .entry(family.into())
                        .or_default()
                        .insert(quick, Rc::clone(&glyphs));
                }
                glyphs
            }
        };
        let baseline = (
            origin.0,
            origin.1 + (metrics.baseline as f32 * scale).round(),
        );
        for glyph in glyphs.iter() {
            let physical = glyph.physical(baseline, scale);
            let Some(image) = self.cell_image(physical.cache_key) else {
                continue;
            };
            out.push(RasterGlyph {
                cache_key: image.hash,
                x: (physical.x + image.left) as f32,
                y: (physical.y - image.top) as f32,
                width: image.width,
                height: image.height,
                draw_width: image.width as f32,
                draw_height: image.height as f32,
                content: image.content,
                tint: None,
                font_size: 0.0,
                data: image.data,
            });
        }
    }

    /// A character shaped on its own, which is where a face that lacks it is
    /// swapped for one that has it. Kept with its pen at zero.
    fn cell_shape(&mut self, key: &CellShapeKey) -> Rc<[LayoutGlyph]> {
        if let Some(glyphs) = self.terminal.shapes.get(key) {
            return Rc::clone(glyphs);
        }
        let size = f32::from_bits(key.size_bits);
        let family = resolve_family(&self.fonts, &key.family);
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(size, size * 1.5));
        let attrs = Attrs::new()
            .family(family.family())
            .weight(if key.bold {
                Weight::BOLD
            } else {
                Weight::NORMAL
            })
            .style(if key.italic {
                Style::Italic
            } else {
                Style::Normal
            });
        buffer.set_text(&key.text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.fonts, false);
        let mut glyphs: Vec<LayoutGlyph> = buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter().cloned())
            .collect();
        // Positions inside the cluster are kept, relative to its first glyph;
        // where the cell is decides the rest.
        let first = glyphs.first().map_or(0.0, |glyph| glyph.x);
        for glyph in &mut glyphs {
            glyph.x -= first;
            glyph.y = 0.0;
        }
        let glyphs: Rc<[LayoutGlyph]> = glyphs.into();
        // A terminal shows few distinct characters, but a program can print
        // any number of them; the cache is dropped rather than left to grow.
        if self.terminal.shapes.len() > 8192 {
            self.terminal.shapes.clear();
            self.terminal.chars.clear();
        }
        self.terminal.shapes.insert(key.clone(), Rc::clone(&glyphs));
        glyphs
    }

    fn cell_image(&mut self, key: CacheKey) -> Option<CellImage> {
        if let Some(image) = self.terminal.images.get(&key) {
            return image.clone();
        }
        let image = self
            .glyphs
            .get_image(&mut self.fonts, key)
            .as_ref()
            .map(|image| {
                let mut hasher = DefaultHasher::new();
                key.hash(&mut hasher);
                CellImage {
                    hash: hasher.finish(),
                    left: image.placement.left,
                    top: image.placement.top,
                    width: image.placement.width,
                    height: image.placement.height,
                    content: match image.content {
                        SwashContent::Color => RasterContent::Color,
                        SwashContent::Mask | SwashContent::SubpixelMask => RasterContent::Mask,
                    },
                    data: Rc::new(image.data.clone()),
                }
            });
        if self.terminal.images.len() > 8192 {
            self.terminal.images.clear();
        }
        self.terminal.images.insert(key, image.clone());
        image
    }

    fn drawn_cell(
        &mut self,
        character: char,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Option<Rc<Vec<u8>>> {
        if !drawn_here(character) {
            return None;
        }
        let key = (character, width, height);
        if let Some(image) = self.terminal.drawn.get(&key) {
            return image.clone();
        }
        let image = draw_cell(character, width, height, scale).map(Rc::new);
        if self.terminal.drawn.len() > 4096 {
            self.terminal.drawn.clear();
        }
        self.terminal.drawn.insert(key, image.clone());
        image
    }
}

/// Whether a character is drawn from the cell's geometry rather than a font.
pub fn drawn_here(character: char) -> bool {
    matches!(character as u32, 0x2500..=0x259f | 0x2800..=0x28ff | 0xe0b0..=0xe0b3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(character: char, width: u32, height: u32) -> Vec<u8> {
        draw_cell(character, width, height, 1.0).expect("drawn")
    }

    #[test]
    fn a_line_runs_from_edge_to_edge() {
        let (w, h) = (8, 17);
        let horizontal = coverage('─', w, h);
        let row = (h - 1) / 2;
        assert!((0..w).all(|x| horizontal[(row * w + x) as usize] == 255));
        let vertical = coverage('│', w, h);
        let column = (w - 1) / 2;
        assert!((0..h).all(|y| vertical[(y * w + column) as usize] == 255));
        // A corner reaches the right and bottom edges, and not the others.
        let corner = coverage('┌', w, h);
        assert_eq!(corner[(row * w + w - 1) as usize], 255);
        assert_eq!(corner[((h - 1) * w + column) as usize], 255);
        assert_eq!(corner[(row * w) as usize], 0);
        assert_eq!(corner[column as usize], 0);
    }

    #[test]
    fn a_rounded_corner_meets_the_straight_lines() {
        let (w, h) = (8, 16);
        let arc = coverage('╭', w, h);
        let row = (h - 1) / 2;
        let column = (w - 1) / 2;
        assert_eq!(
            arc[(row * w + w - 1) as usize],
            255,
            "reaches the right edge"
        );
        assert_eq!(
            arc[((h - 1) * w + column) as usize],
            255,
            "reaches the bottom edge"
        );
        assert_eq!(arc[0], 0, "the corner is cut");
        for corner in ['╮', '╯', '╰'] {
            let arc = coverage(corner, w, h);
            let reaches = |x: u32, y: u32| arc[(y * w + x) as usize] == 255;
            let horizontal = reaches(0, row) || reaches(w - 1, row);
            let vertical = reaches(column, 0) || reaches(column, h - 1);
            assert!(horizontal && vertical, "{corner} reaches its edges");
        }
    }

    #[test]
    fn blocks_and_braille_fill_their_part_of_the_cell() {
        let (w, h) = (8, 16);
        let full = coverage('█', w, h);
        assert!(full.iter().all(|&value| value == 255));
        let lower = coverage('▄', w, h);
        assert_eq!(lower[0], 0);
        assert_eq!(lower[((h - 1) * w) as usize], 255);
        let all_dots = coverage('⣿', w, h);
        let none = coverage('⠀', w, h);
        assert!(none.iter().all(|&value| value == 0));
        let lit = all_dots.iter().filter(|&&value| value > 0).count();
        assert!(lit > 8, "eight dots show: {lit}");
    }

    #[test]
    fn only_the_joining_characters_are_drawn_here() {
        assert!(drawn_here('─') && drawn_here('⣿') && drawn_here('█') && drawn_here('\u{e0b0}'));
        assert!(!drawn_here('a') && !drawn_here('日'));
    }
}
