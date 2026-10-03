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

/// How heavy one arm of a box-drawing character is.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Arm {
    None,
    Light,
    Heavy,
    Double,
}

/// The four arms of a box-drawing character: left, right, up, down.
fn arms(character: char) -> Option<[Arm; 4]> {
    use Arm::{Double as D, Heavy as H, Light as L, None as N};
    // Left, right, up, down, for U+2500 onwards; dashes, arcs and diagonals
    // are drawn separately and are `None` here.
    const TABLE: [[Arm; 4]; 128] = [
        [L, L, N, N],
        [H, H, N, N],
        [N, N, L, L],
        [N, N, H, H], // ─━│┃
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4], // dashes
        [N, L, N, L],
        [N, H, N, L],
        [N, L, N, H],
        [N, H, N, H], // ┌┍┎┏
        [L, N, N, L],
        [H, N, N, L],
        [L, N, N, H],
        [H, N, N, H], // ┐┑┒┓
        [N, L, L, N],
        [N, H, L, N],
        [N, L, H, N],
        [N, H, H, N], // └┕┖┗
        [L, N, L, N],
        [H, N, L, N],
        [L, N, H, N],
        [H, N, H, N], // ┘┙┚┛
        [N, L, L, L],
        [N, H, L, L],
        [N, L, H, L],
        [N, L, L, H], // ├┝┞┟
        [N, L, H, H],
        [N, H, H, L],
        [N, H, L, H],
        [N, H, H, H], // ┠┡┢┣
        [L, N, L, L],
        [H, N, L, L],
        [L, N, H, L],
        [L, N, L, H], // ┤┥┦┧
        [L, N, H, H],
        [H, N, H, L],
        [H, N, L, H],
        [H, N, H, H], // ┨┩┪┫
        [L, L, N, L],
        [H, L, N, L],
        [L, H, N, L],
        [H, H, N, L], // ┬┭┮┯
        [L, L, N, H],
        [H, L, N, H],
        [L, H, N, H],
        [H, H, N, H], // ┰┱┲┳
        [L, L, L, N],
        [H, L, L, N],
        [L, H, L, N],
        [H, H, L, N], // ┴┵┶┷
        [L, L, H, N],
        [H, L, H, N],
        [L, H, H, N],
        [H, H, H, N], // ┸┹┺┻
        [L, L, L, L],
        [H, L, L, L],
        [L, H, L, L],
        [H, H, L, L], // ┼┽┾┿
        [L, L, H, L],
        [L, L, L, H],
        [L, L, H, H],
        [H, L, H, L], // ╀╁╂╃
        [L, H, H, L],
        [H, L, L, H],
        [L, H, L, H],
        [H, H, H, L], // ╄╅╆╇
        [H, H, L, H],
        [H, L, H, H],
        [L, H, H, H],
        [H, H, H, H], // ╈╉╊╋
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4], // ╌╍╎╏
        [D, D, N, N],
        [N, N, D, D], // ═║
        [N, D, N, L],
        [N, L, N, D],
        [N, D, N, D], // ╒╓╔
        [D, N, N, L],
        [L, N, N, D],
        [D, N, N, D], // ╕╖╗
        [N, D, L, N],
        [N, L, D, N],
        [N, D, D, N], // ╘╙╚
        [D, N, L, N],
        [L, N, D, N],
        [D, N, D, N], // ╛╜╝
        [N, D, L, L],
        [N, L, D, D],
        [N, D, D, D], // ╞╟╠
        [D, N, L, L],
        [L, N, D, D],
        [D, N, D, D], // ╡╢╣
        [D, D, N, L],
        [L, L, N, D],
        [D, D, N, D], // ╤╥╦
        [D, D, L, N],
        [L, L, D, N],
        [D, D, D, N], // ╧╨╩
        [D, D, L, L],
        [L, L, D, D],
        [D, D, D, D], // ╪╫╬
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4], // ╭╮╯╰
        [N; 4],
        [N; 4],
        [N; 4], // ╱╲╳
        [L, N, N, N],
        [N, N, L, N],
        [N, L, N, N],
        [N, N, N, L], // ╴╵╶╷
        [H, N, N, N],
        [N, N, H, N],
        [N, H, N, N],
        [N, N, N, H], // ╸╹╺╻
        [L, H, N, N],
        [N, N, L, H],
        [H, L, N, N],
        [N, N, H, L], // ╼╽╾╿
    ];
    let index = (character as u32).checked_sub(0x2500)? as usize;
    let arms = *TABLE.get(index)?;
    (arms != [N; 4]).then_some(arms)
}

/// Coverage of one cell, drawn from its geometry: a line runs to the cell's
/// edge, so it meets the next cell's on the same pixel.
struct Canvas {
    width: u32,
    height: u32,
    /// Coverage, zero to one, per pixel.
    pixels: Vec<f32>,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0.0; (width * height) as usize],
        }
    }

    /// Fills whole pixels in `[x0, x1) × [y0, y1)`, clamped to the cell.
    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, alpha: f32) {
        let clamp_x = |value: i32| value.clamp(0, self.width as i32) as u32;
        let clamp_y = |value: i32| value.clamp(0, self.height as i32) as u32;
        for y in clamp_y(y0)..clamp_y(y1) {
            for x in clamp_x(x0)..clamp_x(x1) {
                let pixel = &mut self.pixels[(y * self.width + x) as usize];
                *pixel = pixel.max(alpha);
            }
        }
    }

    /// Fills where `inside` holds, sampled four by four per pixel so a curve
    /// or a slope has a soft edge.
    fn shape(&mut self, inside: impl Fn(f32, f32) -> bool) {
        const SAMPLES: u32 = 4;
        for y in 0..self.height {
            for x in 0..self.width {
                let mut hits = 0;
                for sy in 0..SAMPLES {
                    for sx in 0..SAMPLES {
                        let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                        let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                        if inside(px, py) {
                            hits += 1;
                        }
                    }
                }
                if hits > 0 {
                    let pixel = &mut self.pixels[(y * self.width + x) as usize];
                    *pixel = pixel.max(hits as f32 / (SAMPLES * SAMPLES) as f32);
                }
            }
        }
    }

    fn finish(self) -> Vec<u8> {
        self.pixels
            .into_iter()
            .map(|coverage| (coverage.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect()
    }
}

/// Draws one of the characters [`drawn_here`] names, `width` × `height`
/// device pixels, as coverage.
fn draw_cell(character: char, width: u32, height: u32, scale: f32) -> Option<Vec<u8>> {
    let mut canvas = Canvas::new(width, height);
    let (w, h) = (width as i32, height as i32);
    // A light line is a device pixel per logical one, at least one; a heavy
    // one twice that.
    let light = (scale.round() as i32).max(1).min(w.max(1));
    let heavy = (light * 2).min(w.max(1));
    // The band of a line of `thickness` centred in `extent`.
    let band = |extent: i32, thickness: i32| {
        let start = (extent - thickness) / 2;
        (start, start + thickness)
    };
    let code = character as u32;
    match code {
        0x2500..=0x257f => {
            if let Some(arms) = arms(character) {
                draw_arms(&mut canvas, arms, light, heavy);
            } else {
                match code {
                    // Dashes: three, four or two segments, light or heavy.
                    0x2504..=0x250b | 0x254c..=0x254f => {
                        let (count, horizontal, heavy_line) = match code {
                            0x2504 => (3, true, false),
                            0x2505 => (3, true, true),
                            0x2506 => (3, false, false),
                            0x2507 => (3, false, true),
                            0x2508 => (4, true, false),
                            0x2509 => (4, true, true),
                            0x250a => (4, false, false),
                            0x250b => (4, false, true),
                            0x254c => (2, true, false),
                            0x254d => (2, true, true),
                            0x254e => (2, false, false),
                            _ => (2, false, true),
                        };
                        let thickness = if heavy_line { heavy } else { light };
                        let extent = if horizontal { w } else { h };
                        let (b0, b1) = band(if horizontal { h } else { w }, thickness);
                        for index in 0..count {
                            let start = extent * index / count;
                            let end = extent * (index + 1) / count;
                            let gap = ((end - start) / 4).max(1);
                            let (s0, s1) = (start + gap / 2, end - (gap - gap / 2));
                            if horizontal {
                                canvas.rect(s0, b0, s1, b1, 1.0);
                            } else {
                                canvas.rect(b0, s0, b1, s1, 1.0);
                            }
                        }
                    }
                    // Rounded corners: a quarter circle joining the two
                    // centre lines, and straight on to the edges.
                    0x256d..=0x2570 => {
                        let (x0, x1) = band(w, light);
                        let (y0, y1) = band(h, light);
                        let cx = (x0 + x1) as f32 / 2.0;
                        let cy = (y0 + y1) as f32 / 2.0;
                        // To the nearer edge, less half a pixel, so the
                        // arc ends inside the cell and a straight piece
                        // carries the line the rest of the way.
                        let radius =
                            (cx.min(w as f32 - cx).min(cy).min(h as f32 - cy) - 0.5).max(1.0);
                        let half = light as f32 / 2.0;
                        // Which way the arms go: right or left, down or up.
                        let (right, down) = match code {
                            0x256d => (true, true),
                            0x256e => (false, true),
                            0x256f => (false, false),
                            _ => (true, false),
                        };
                        let centre_x = if right { cx + radius } else { cx - radius };
                        let centre_y = if down { cy + radius } else { cy - radius };
                        canvas.shape(|px, py| {
                            let in_quadrant =
                                (if right {
                                    px <= centre_x
                                } else {
                                    px >= centre_x
                                }) && (if down { py <= centre_y } else { py >= centre_y });
                            in_quadrant && {
                                let distance =
                                    ((px - centre_x).powi(2) + (py - centre_y).powi(2)).sqrt();
                                (distance - radius).abs() <= half
                            }
                        });
                        let arc_x = centre_x.floor() as i32;
                        let arc_y = centre_y.floor() as i32;
                        if right {
                            canvas.rect(arc_x, y0, w, y1, 1.0);
                        } else {
                            canvas.rect(0, y0, arc_x + 1, y1, 1.0);
                        }
                        if down {
                            canvas.rect(x0, arc_y, x1, h, 1.0);
                        } else {
                            canvas.rect(x0, 0, x1, arc_y + 1, 1.0);
                        }
                    }
                    // Diagonals.
                    0x2571..=0x2573 => {
                        let half = light as f32 / 2.0 * 1.2;
                        let (wf, hf) = (w as f32, h as f32);
                        let length = (wf * wf + hf * hf).sqrt();
                        let rising = move |px: f32, py: f32| {
                            ((hf * px + wf * py - wf * hf) / length).abs() <= half
                        };
                        let falling =
                            move |px: f32, py: f32| ((hf * px - wf * py) / length).abs() <= half;
                        match code {
                            0x2571 => canvas.shape(rising),
                            0x2572 => canvas.shape(falling),
                            _ => canvas.shape(|px, py| rising(px, py) || falling(px, py)),
                        }
                    }
                    _ => return None,
                }
            }
        }
        // Block elements.
        0x2580..=0x259f => {
            let eighth_h = |n: i32| (h * n + 4) / 8;
            let eighth_w = |n: i32| (w * n + 4) / 8;
            let (half_w, half_h) = (w / 2, h / 2);
            match code {
                0x2580 => canvas.rect(0, 0, w, half_h, 1.0),
                0x2581..=0x2588 => {
                    let n = (code - 0x2580) as i32;
                    canvas.rect(0, h - eighth_h(n), w, h, 1.0);
                }
                0x2589..=0x258f => {
                    let n = 8 - (code - 0x2588) as i32;
                    canvas.rect(0, 0, eighth_w(n), h, 1.0);
                }
                0x2590 => canvas.rect(half_w, 0, w, h, 1.0),
                0x2591 => canvas.rect(0, 0, w, h, 0.25),
                0x2592 => canvas.rect(0, 0, w, h, 0.5),
                0x2593 => canvas.rect(0, 0, w, h, 0.75),
                0x2594 => canvas.rect(0, 0, w, eighth_h(1), 1.0),
                0x2595 => canvas.rect(w - eighth_w(1), 0, w, h, 1.0),
                _ => {
                    // Quadrants: upper left, upper right, lower left, lower right.
                    let quadrants: [bool; 4] = match code {
                        0x2596 => [false, false, true, false],
                        0x2597 => [false, false, false, true],
                        0x2598 => [true, false, false, false],
                        0x2599 => [true, false, true, true],
                        0x259a => [true, false, false, true],
                        0x259b => [true, true, true, false],
                        0x259c => [true, true, false, true],
                        0x259d => [false, true, false, false],
                        0x259e => [false, true, true, false],
                        _ => [false, true, true, true],
                    };
                    let boxes = [
                        (0, 0, half_w, half_h),
                        (half_w, 0, w, half_h),
                        (0, half_h, half_w, h),
                        (half_w, half_h, w, h),
                    ];
                    for (on, (x0, y0, x1, y1)) in quadrants.into_iter().zip(boxes) {
                        if on {
                            canvas.rect(x0, y0, x1, y1, 1.0);
                        }
                    }
                }
            }
        }
        // Braille: two columns of four dots, round, spread over the cell.
        0x2800..=0x28ff => {
            let bits = code - 0x2800;
            if bits == 0 {
                return Some(canvas.finish());
            }
            // Dot n's column and row, as the pattern numbers them.
            const DOTS: [(u32, u32); 8] = [
                (0, 0),
                (0, 1),
                (0, 2),
                (1, 0),
                (1, 1),
                (1, 2),
                (0, 3),
                (1, 3),
            ];
            let (wf, hf) = (w as f32, h as f32);
            let radius = (wf / 4.0).min(hf / 8.0) * 0.8;
            let centres: Vec<(f32, f32)> = DOTS
                .iter()
                .enumerate()
                .filter(|(index, _)| bits & (1 << index) != 0)
                .map(|(_, (column, row))| {
                    (
                        wf * (1.0 + 2.0 * *column as f32) / 4.0,
                        hf * (1.0 + 2.0 * *row as f32) / 8.0,
                    )
                })
                .collect();
            canvas.shape(|px, py| {
                centres
                    .iter()
                    .any(|(cx, cy)| (px - cx).powi(2) + (py - cy).powi(2) <= radius * radius)
            });
        }
        // Powerline: a solid triangle or a chevron, pointing right or left.
        0xe0b0..=0xe0b3 => {
            let (wf, hf) = (w as f32, h as f32);
            let half = light as f32 / 2.0 * 1.4;
            match code {
                0xe0b0 => canvas.shape(|px, py| px / wf <= 1.0 - (2.0 * py / hf - 1.0).abs()),
                0xe0b2 => {
                    canvas.shape(|px, py| (wf - px) / wf <= 1.0 - (2.0 * py / hf - 1.0).abs())
                }
                0xe0b1 | 0xe0b3 => {
                    let left = code == 0xe0b3;
                    canvas.shape(|px, py| {
                        let x = if left { wf - px } else { px };
                        // The two strokes from the corners to the middle.
                        let along = if py <= hf / 2.0 { py } else { hf - py };
                        let target = along / (hf / 2.0) * wf;
                        (x - target).abs() <= half * (1.0 + (wf / hf) * 2.0).min(3.0)
                    });
                }
                _ => return None,
            }
        }
        _ => return None,
    }
    Some(canvas.finish())
}

/// A box-drawing character from its four arms, each running from the cell's
/// centre to its edge, joined where they meet.
fn draw_arms(canvas: &mut Canvas, arms: [Arm; 4], light: i32, heavy: i32) {
    let (w, h) = (canvas.width as i32, canvas.height as i32);
    let [left, right, up, down] = arms;
    let thickness = |arm: Arm| match arm {
        Arm::None => 0,
        Arm::Light | Arm::Double => light,
        Arm::Heavy => heavy,
    };
    let band = |extent: i32, thickness: i32| {
        let start = (extent - thickness) / 2;
        (start, start + thickness)
    };
    // How far the crossing reaches, so a horizontal arm runs into the widest
    // vertical one rather than stopping at the centre.
    let vertical = thickness(up).max(thickness(down)).max(light);
    let horizontal = thickness(left).max(thickness(right)).max(light);
    let (vx0, vx1) = band(w, vertical);
    let (hy0, hy1) = band(h, horizontal);
    let double_gap = light;
    // A double line is two light lines either side of the centre line.
    let horizontal_bands = |arm: Arm| -> Vec<(i32, i32)> {
        match arm {
            Arm::None => Vec::new(),
            Arm::Double => {
                let (c0, c1) = band(h, light);
                vec![
                    (c0 - double_gap - light, c1 - double_gap - light),
                    (c0 + double_gap + light, c1 + double_gap + light),
                ]
            }
            arm => vec![band(h, thickness(arm))],
        }
    };
    let vertical_bands = |arm: Arm| -> Vec<(i32, i32)> {
        match arm {
            Arm::None => Vec::new(),
            Arm::Double => {
                let (c0, c1) = band(w, light);
                vec![
                    (c0 - double_gap - light, c1 - double_gap - light),
                    (c0 + double_gap + light, c1 + double_gap + light),
                ]
            }
            arm => vec![band(w, thickness(arm))],
        }
    };
    let doubled = arms.contains(&Arm::Double);
    // With doubles, arms meet at the outer edge of the double bands.
    let reach_x = if doubled {
        (vx0 - double_gap - light, vx1 + double_gap + light)
    } else {
        (vx0, vx1)
    };
    let reach_y = if doubled {
        (hy0 - double_gap - light, hy1 + double_gap + light)
    } else {
        (hy0, hy1)
    };
    for (y0, y1) in horizontal_bands(left) {
        canvas.rect(0, y0, reach_x.1, y1, 1.0);
    }
    for (y0, y1) in horizontal_bands(right) {
        canvas.rect(reach_x.0, y0, w, y1, 1.0);
    }
    for (x0, x1) in vertical_bands(up) {
        canvas.rect(x0, 0, x1, reach_y.1, 1.0);
    }
    for (x0, x1) in vertical_bands(down) {
        canvas.rect(x0, reach_y.0, x1, h, 1.0);
    }
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
