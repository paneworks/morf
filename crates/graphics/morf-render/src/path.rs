//! `Path`: SVG path data, drawn at the pixel size it covers.
//!
//! The outline is geometry until the last moment: parsed once, morphed and
//! trimmed as curves, and rasterised by tiny-skia at exactly the device pixels
//! the node covers on screen — its layout size, times the output scale, times
//! whatever scale its transform adds. So it is as sharp at 4x as at 1x, and a
//! number that moves (a trim, a width, a colour, a morph) draws the next frame
//! from the curves rather than stretching the last one.
//!
//! What was drawn is kept, keyed by every input that shapes the pixels, so a
//! path that is not moving costs a texture lookup and nothing else.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use kurbo::{BezPath, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point};
use morf_scene::{Color, FillRule, PathViewBox, StrokeCap, StrokeJoin};
use resvg::tiny_skia;

use crate::ImageFillMode;

/// Everything a path's pixels depend on besides where it is on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct PathPaint {
    /// SVG path data.
    pub d: String,
    /// The outline it is morphing into, empty when it is not.
    pub morph_to: String,
    /// How far along the morph, zero at `d`.
    pub morph_progress: f64,
    pub fill_color: Color,
    pub fill_rule: FillRule,
    pub stroke_color: Color,
    /// In path units, which the view box scales like everything else.
    pub stroke_width: f64,
    pub stroke_cap: StrokeCap,
    pub stroke_join: StrokeJoin,
    pub miter_limit: f64,
    /// Dash and gap lengths in path units; empty is a solid stroke.
    pub dash: Vec<f64>,
    pub dash_offset: f64,
    /// The stroked part of the outline, as fractions of its length.
    pub trim_start: f64,
    pub trim_end: f64,
    /// The rectangle of path space stretched over the node, if any.
    pub view_box: Option<PathViewBox>,
    pub fill_mode: ImageFillMode,
}

impl PathPaint {
    fn strokes(&self) -> bool {
        self.stroke_color.alpha > 0.0
            && self.stroke_width > 0.0
            && self.trim_end > self.trim_start
            && self.trim_end > 0.0
            && self.trim_start < 1.0
    }

    /// Path units to the node's logical pixels, as scale and offset per axis.
    pub(crate) fn to_node(&self, width: f64, height: f64) -> ([f64; 2], [f64; 2]) {
        let Some(view_box) = self.view_box else {
            return ([1.0, 1.0], [0.0, 0.0]);
        };
        let fit_x = width / view_box.width;
        let fit_y = height / view_box.height;
        let (scale_x, scale_y) = match self.fill_mode {
            ImageFillMode::Stretch => (fit_x, fit_y),
            ImageFillMode::PreserveAspectFit => (fit_x.min(fit_y), fit_x.min(fit_y)),
            ImageFillMode::PreserveAspectCrop => (fit_x.max(fit_y), fit_x.max(fit_y)),
        };
        // Centred, as SVG's default `xMidYMid` centres a box that does not
        // fill the viewport.
        let offset_x = (width - view_box.width * scale_x) / 2.0 - view_box.x * scale_x;
        let offset_y = (height - view_box.height * scale_y) / 2.0 - view_box.y * scale_y;
        ([scale_x, scale_y], [offset_x, offset_y])
    }

    /// How far past the node's box the drawing may reach, in logical pixels:
    /// half a stroke, more at a mitred corner or a square cap, and a pixel of
    /// antialiasing.
    pub(crate) fn margin(&self, width: f64, height: f64) -> f64 {
        let mut margin = 1.0;
        if self.strokes() {
            let ([scale_x, scale_y], _) = self.to_node(width, height);
            let half = self.stroke_width * scale_x.max(scale_y) / 2.0;
            let reach = match (self.stroke_join, self.stroke_cap) {
                (StrokeJoin::Miter, _) => self.miter_limit.max(1.0),
                (_, StrokeCap::Square) => std::f64::consts::SQRT_2,
                _ => 1.0,
            };
            margin += half * reach;
        }
        margin.min(256.0)
    }

    /// A key for everything that shapes the pixels, at a pixel size.
    pub(crate) fn key(&self, pixels: (u32, u32), logical: (f64, f64)) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.d.hash(&mut hasher);
        self.morph_to.hash(&mut hasher);
        for number in [
            self.morph_progress,
            self.stroke_width,
            self.miter_limit,
            self.dash_offset,
            self.trim_start,
            self.trim_end,
            logical.0,
            logical.1,
        ] {
            number.to_bits().hash(&mut hasher);
        }
        for color in [self.fill_color, self.stroke_color] {
            for channel in [color.red, color.green, color.blue, color.alpha] {
                channel.to_bits().hash(&mut hasher);
            }
        }
        self.fill_rule.hash(&mut hasher);
        self.stroke_cap.hash(&mut hasher);
        self.stroke_join.hash(&mut hasher);
        for length in &self.dash {
            length.to_bits().hash(&mut hasher);
        }
        if let Some(view_box) = self.view_box {
            for number in [view_box.x, view_box.y, view_box.width, view_box.height] {
                number.to_bits().hash(&mut hasher);
            }
        }
        (self.fill_mode as u8).hash(&mut hasher);
        pixels.hash(&mut hasher);
        hasher.finish()
    }
}

/// Parsed outlines, by their path data, so a static `d` is read once.
#[derive(Default)]
pub(crate) struct PathOutlines {
    parsed: HashMap<String, Option<BezPath>>,
}

/// How many distinct path strings are kept parsed. A morph that rewrites `d`
/// every frame would otherwise grow this without end.
const MAX_OUTLINES: usize = 512;

impl PathOutlines {
    fn parse(&mut self, data: &str) -> Option<BezPath> {
        if let Some(parsed) = self.parsed.get(data) {
            return parsed.clone();
        }
        if self.parsed.len() >= MAX_OUTLINES {
            self.parsed.clear();
        }
        let parsed = (!data.trim().is_empty())
            .then(|| BezPath::from_svg(data).ok())
            .flatten();
        self.parsed.insert(data.to_owned(), parsed.clone());
        parsed
    }

    /// The outline at this point of its morph, in path units.
    pub(crate) fn outline(&mut self, paint: &PathPaint) -> Option<BezPath> {
        let from = self.parse(&paint.d);
        let progress = paint.morph_progress.clamp(0.0, 1.0);
        if paint.morph_to.is_empty() || progress <= 0.0 {
            return from;
        }
        let to = self.parse(&paint.morph_to);
        match (from, to) {
            (Some(from), Some(to)) => Some(morph(&from, &to, progress)),
            (from, to) if progress < 0.5 => from.or(to),
            (from, to) => to.or(from),
        }
    }
}

/// Every segment as a cubic, so a line in one outline can meet a curve in the
/// other: a line is the cubic with its handles a third of the way along, and a
/// quadratic raises to a cubic exactly.
fn cubics(path: &BezPath) -> Vec<PathEl> {
    let mut out = Vec::with_capacity(path.elements().len());
    let mut last = Point::ORIGIN;
    let mut start = Point::ORIGIN;
    for element in path.iter() {
        match element {
            PathEl::MoveTo(point) => {
                out.push(PathEl::MoveTo(point));
                last = point;
                start = point;
            }
            PathEl::LineTo(point) => {
                out.push(PathEl::CurveTo(
                    last.lerp(point, 1.0 / 3.0),
                    last.lerp(point, 2.0 / 3.0),
                    point,
                ));
                last = point;
            }
            PathEl::QuadTo(control, point) => {
                out.push(PathEl::CurveTo(
                    last.lerp(control, 2.0 / 3.0),
                    point.lerp(control, 2.0 / 3.0),
                    point,
                ));
                last = point;
            }
            PathEl::CurveTo(first, second, point) => {
                out.push(PathEl::CurveTo(first, second, point));
                last = point;
            }
            PathEl::ClosePath => {
                out.push(PathEl::ClosePath);
                last = start;
            }
        }
    }
    out
}

/// Walks one outline onto another point by point.
///
/// Only when the two have the same run of moves, segments and closes — the
/// same drawing with its points moved, which is what a face's mouth turning
/// from a smile to a frown is. Anything else has no point-to-point
/// correspondence to walk, so the outline changes over at the halfway mark.
pub(crate) fn morph(from: &BezPath, to: &BezPath, progress: f64) -> BezPath {
    let a = cubics(from);
    let b = cubics(to);
    let same_shape = a.len() == b.len()
        && a.iter()
            .zip(&b)
            .all(|(a, b)| std::mem::discriminant(a) == std::mem::discriminant(b));
    if !same_shape {
        return if progress < 0.5 {
            from.clone()
        } else {
            to.clone()
        };
    }
    let lerp = |a: Point, b: Point| a.lerp(b, progress);
    BezPath::from_vec(
        a.iter()
            .zip(&b)
            .map(|pair| match pair {
                (PathEl::MoveTo(a), PathEl::MoveTo(b)) => PathEl::MoveTo(lerp(*a, *b)),
                (PathEl::CurveTo(a1, a2, a3), PathEl::CurveTo(b1, b2, b3)) => {
                    PathEl::CurveTo(lerp(*a1, *b1), lerp(*a2, *b2), lerp(*a3, *b3))
                }
                _ => PathEl::ClosePath,
            })
            .collect(),
    )
}

/// The part of an outline between two fractions of its whole length.
///
/// Measured across every subpath in order, so a trim over a drawing of
/// several strokes draws them one after another.
pub(crate) fn trim(path: &BezPath, start: f64, end: f64) -> BezPath {
    let start = start.clamp(0.0, 1.0);
    let end = end.clamp(0.0, 1.0);
    if start <= 0.0 && end >= 1.0 {
        return path.clone();
    }
    const ACCURACY: f64 = 1e-3;
    let segments: Vec<(PathSeg, f64)> = path
        .segments()
        .map(|segment| (segment, segment.arclen(ACCURACY)))
        .collect();
    let total: f64 = segments.iter().map(|(_, length)| length).sum();
    let mut out = BezPath::new();
    if total <= 0.0 || end <= start {
        return out;
    }
    let (from, to) = (start * total, end * total);
    let mut walked = 0.0;
    let mut pen: Option<Point> = None;
    for (segment, length) in segments {
        let (begin, finish) = (walked, walked + length);
        walked = finish;
        if finish <= from || begin >= to || length <= 0.0 {
            continue;
        }
        let t0 = if from > begin {
            segment.inv_arclen(from - begin, ACCURACY)
        } else {
            0.0
        };
        let t1 = if to < finish {
            segment.inv_arclen(to - begin, ACCURACY)
        } else {
            1.0
        };
        let piece = segment.subsegment(t0..t1);
        let head = piece.start();
        if pen.is_none_or(|pen| pen.distance(head) > 1e-9) {
            out.move_to(head);
        }
        match piece {
            PathSeg::Line(line) => out.line_to(line.p1),
            PathSeg::Quad(quad) => out.quad_to(quad.p1, quad.p2),
            PathSeg::Cubic(cubic) => out.curve_to(cubic.p1, cubic.p2, cubic.p3),
        }
        pen = Some(piece.end());
    }
    out
}

fn skia_path(path: &BezPath) -> Option<tiny_skia::Path> {
    let mut builder = tiny_skia::PathBuilder::new();
    let point = |point: Point| (point.x as f32, point.y as f32);
    for element in path.iter() {
        match element {
            PathEl::MoveTo(p) => {
                let (x, y) = point(p);
                builder.move_to(x, y);
            }
            PathEl::LineTo(p) => {
                let (x, y) = point(p);
                builder.line_to(x, y);
            }
            PathEl::QuadTo(c, p) => {
                let ((cx, cy), (x, y)) = (point(c), point(p));
                builder.quad_to(cx, cy, x, y);
            }
            PathEl::CurveTo(c1, c2, p) => {
                let ((ax, ay), (bx, by), (x, y)) = (point(c1), point(c2), point(p));
                builder.cubic_to(ax, ay, bx, by, x, y);
            }
            PathEl::ClosePath => builder.close(),
        }
    }
    builder.finish()
}

fn skia_color(color: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(
        color.red.clamp(0.0, 1.0),
        color.green.clamp(0.0, 1.0),
        color.blue.clamp(0.0, 1.0),
        color.alpha.clamp(0.0, 1.0),
    )
    .unwrap_or(tiny_skia::Color::TRANSPARENT)
}

/// One path drawn into straight-alpha RGBA pixels.
pub(crate) struct PathPixels {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

/// Draws `paint` into a `pixels`-sized image covering the node's box, grown by
/// `margin` logical pixels on every side.
pub(crate) fn rasterize(
    outlines: &mut PathOutlines,
    paint: &PathPaint,
    logical: (f64, f64),
    margin: f64,
    pixels: (u32, u32),
) -> Option<PathPixels> {
    let outline = outlines.outline(paint)?;
    let (width, height) = pixels;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    let ([scale_x, scale_y], [offset_x, offset_y]) = paint.to_node(logical.0, logical.1);
    // Logical to device pixels, exactly: the image is a whole number of pixels
    // and the box it covers is not.
    let device_x = f64::from(width) / (logical.0 + margin * 2.0);
    let device_y = f64::from(height) / (logical.1 + margin * 2.0);
    let transform = tiny_skia::Transform::from_row(
        (scale_x * device_x) as f32,
        0.0,
        0.0,
        (scale_y * device_y) as f32,
        ((offset_x + margin) * device_x) as f32,
        ((offset_y + margin) * device_y) as f32,
    );
    if paint.fill_color.alpha > 0.0
        && let Some(path) = skia_path(&outline)
    {
        let mut fill = tiny_skia::Paint::default();
        fill.set_color(skia_color(paint.fill_color));
        fill.anti_alias = true;
        let rule = match paint.fill_rule {
            FillRule::NonZero => tiny_skia::FillRule::Winding,
            FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
        };
        pixmap.fill_path(&path, &fill, rule, transform, None);
    }
    if paint.strokes()
        && let Some(path) = skia_path(&trim(&outline, paint.trim_start, paint.trim_end))
    {
        let mut ink = tiny_skia::Paint::default();
        ink.set_color(skia_color(paint.stroke_color));
        ink.anti_alias = true;
        // An odd dash list is read twice over, as SVG reads it; a list with
        // no length in it is a solid stroke.
        let mut dash = paint.dash.clone();
        if dash.len() % 2 == 1 {
            dash.extend_from_within(..);
        }
        let dash = (dash.iter().sum::<f64>() > 0.0)
            .then(|| {
                tiny_skia::StrokeDash::new(
                    dash.iter().map(|length| *length as f32).collect(),
                    paint.dash_offset as f32,
                )
            })
            .flatten();
        let stroke = tiny_skia::Stroke {
            width: paint.stroke_width as f32,
            miter_limit: paint.miter_limit.max(1.0) as f32,
            line_cap: match paint.stroke_cap {
                StrokeCap::Butt => tiny_skia::LineCap::Butt,
                StrokeCap::Round => tiny_skia::LineCap::Round,
                StrokeCap::Square => tiny_skia::LineCap::Square,
            },
            line_join: match paint.stroke_join {
                StrokeJoin::Miter => tiny_skia::LineJoin::Miter,
                StrokeJoin::Round => tiny_skia::LineJoin::Round,
                StrokeJoin::Bevel => tiny_skia::LineJoin::Bevel,
            },
            dash,
        };
        pixmap.stroke_path(&path, &ink, &stroke, transform, None);
    }
    let mut rgba = pixmap.take();
    // The texture pipeline takes straight alpha, as a decoded image is.
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        #[allow(unknown_lints, clippy::manual_checked_ops)]
        if alpha != 0 {
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
    Some(PathPixels {
        width,
        height,
        rgba,
    })
}
