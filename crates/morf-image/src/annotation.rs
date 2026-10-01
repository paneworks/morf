//! Typed annotation geometry. Lua supplies numbers, never an SVG document.
//! Only font shaping uses resvg; drawing tools go straight to tiny-skia and
//! allocate a tile around the mark rather than a desktop-sized overlay.
use crate::{ImageError, image_cache::decode_svg};
use image::{DynamicImage, RgbaImage};
use resvg::tiny_skia::{self, FillRule, Paint, PathBuilder, Stroke, Transform};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Rect,
    Ellipse,
    Line,
    Arrow,
    Pen,
    Marker,
    Text,
    Step,
    Zoom,
    Blur,
    Pixelate,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub kind: Kind,
    pub points: Vec<Point>,
    pub color: [u8; 3],
    pub width: f32,
    pub filled: bool,
    pub text: String,
    pub font: String,
    pub number: u32,
}

impl Annotation {
    fn endpoints(&self) -> (Point, Point) {
        (self.points[0], *self.points.last().unwrap())
    }
    fn rect(&self) -> (f32, f32, f32, f32) {
        let (p, q) = self.endpoints();
        (
            p.x.min(q.x),
            p.y.min(q.y),
            (q.x - p.x).abs(),
            (q.y - p.y).abs(),
        )
    }
    /// Geometry shared by the live path and the exported artwork.
    pub fn paths(&self) -> (Option<tiny_skia::Path>, Option<tiny_skia::Path>) {
        if self.points.is_empty() {
            return (None, None);
        }
        let (x, y, w, h) = self.rect();
        let (p, q) = self.endpoints();
        let mut stroke = PathBuilder::new();
        let mut fill = PathBuilder::new();
        match self.kind {
            Kind::Rect | Kind::Marker | Kind::Zoom => {
                if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w, h) {
                    if self.kind == Kind::Marker || (self.filled && self.kind != Kind::Zoom) {
                        fill.push_rect(rect);
                    }
                    if self.kind != Kind::Marker {
                        stroke.push_rect(rect);
                    }
                }
            }
            Kind::Ellipse => {
                if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w, h) {
                    stroke.push_oval(rect);
                    if self.filled {
                        fill.push_oval(rect);
                    }
                }
            }
            Kind::Line | Kind::Arrow | Kind::Pen => {
                stroke.move_to(p.x, p.y);
                for point in &self.points[1..] {
                    stroke.line_to(point.x, point.y);
                }
                if self.kind == Kind::Arrow {
                    let angle = (q.y - p.y).atan2(q.x - p.x);
                    let len = 22.0_f32.max(self.width * 5.0);
                    fill.move_to(q.x, q.y);
                    fill.line_to(
                        q.x - len * (angle - 0.45).cos(),
                        q.y - len * (angle - 0.45).sin(),
                    );
                    fill.line_to(
                        q.x - len * (angle + 0.45).cos(),
                        q.y - len * (angle + 0.45).sin(),
                    );
                    fill.close();
                }
            }
            Kind::Step => {
                fill.push_circle(p.x, p.y, self.width * 3.0);
            }
            Kind::Text | Kind::Blur | Kind::Pixelate => {}
        }
        (stroke.finish(), fill.finish())
    }

    /// Path syntax for the existing native Path node; no XML or SVG tree.
    pub fn path_data(&self) -> (String, String) {
        fn data(path: Option<tiny_skia::Path>) -> String {
            let mut out = String::new();
            if let Some(path) = path {
                for segment in path.segments() {
                    use tiny_skia::PathSegment::*;
                    match segment {
                        MoveTo(p) => {
                            let _ = write!(out, "M{} {}", p.x, p.y);
                        }
                        LineTo(p) => {
                            let _ = write!(out, "L{} {}", p.x, p.y);
                        }
                        QuadTo(p, q) => {
                            let _ = write!(out, "Q{} {} {} {}", p.x, p.y, q.x, q.y);
                        }
                        CubicTo(p, q, r) => {
                            let _ = write!(out, "C{} {} {} {} {} {}", p.x, p.y, q.x, q.y, r.x, r.y);
                        }
                        Close => out.push('Z'),
                    }
                }
            }
            out
        }
        let (stroke, fill) = self.paths();
        (data(stroke), data(fill))
    }
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn draw(image: DynamicImage, marks: &[Annotation]) -> Result<DynamicImage, ImageError> {
    if marks.len() > 129
        || marks.iter().any(|a| {
            a.points.len() < 2
                || a.points.len() > 4096
                || !(1.0..=128.0).contains(&a.width)
                || a.points
                    .iter()
                    .any(|p| !p.x.is_finite() || !p.y.is_finite())
        })
    {
        return Err(ImageError::Refused(
            "invalid annotation geometry or count".into(),
        ));
    }
    let mut base = image.into_rgba8();
    for mark in marks {
        let (stroke, fill) = mark.paths();
        let p = mark.points[0];
        let bounds = stroke.as_ref().or(fill.as_ref()).map(|p| p.bounds());
        let pad = mark.width + 2.0;
        let (left, top, right, bottom) = if mark.kind == Kind::Text {
            (
                p.x - pad,
                p.y - pad,
                p.x + mark.text.chars().count() as f32 * mark.width * 2.0 + pad,
                p.y + mark.width * 1.6 + pad,
            )
        } else if let Some(bounds) = bounds {
            let extra = if mark.kind == Kind::Arrow {
                mark.width * 5.0 + 22.0
            } else {
                pad
            };
            (
                bounds.left() - extra,
                bounds.top() - extra,
                bounds.right() + extra,
                bounds.bottom() + extra,
            )
        } else {
            continue;
        };
        let x = left.floor().clamp(0.0, base.width() as f32) as u32;
        let y = top.floor().clamp(0.0, base.height() as f32) as u32;
        let right = right.ceil().clamp(0.0, base.width() as f32) as u32;
        let bottom = bottom.ceil().clamp(0.0, base.height() as f32) as u32;
        let (w, h) = (right.saturating_sub(x), bottom.saturating_sub(y));
        if w == 0 || h == 0 {
            continue;
        }
        let mut tile = tiny_skia::Pixmap::new(w, h).ok_or(ImageError::InvalidSize)?;
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            mark.color[0],
            mark.color[1],
            mark.color[2],
            if mark.kind == Kind::Marker { 82 } else { 255 },
        );
        let transform = Transform::from_translate(-(x as f32), -(y as f32));
        if let Some(fill) = fill {
            tile.fill_path(&fill, &paint, FillRule::Winding, transform, None);
        }
        if let Some(stroke) = stroke {
            tile.stroke_path(
                &stroke,
                &paint,
                &Stroke {
                    width: mark.width,
                    line_cap: tiny_skia::LineCap::Round,
                    line_join: tiny_skia::LineJoin::Round,
                    ..Stroke::default()
                },
                transform,
                None,
            );
        }
        let mut rgba = Vec::with_capacity((w as usize) * (h as usize) * 4);
        for pixel in tile.pixels() {
            let c = pixel.demultiply();
            rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        let mut overlay = RgbaImage::from_raw(w, h, rgba).ok_or(ImageError::InvalidSize)?;
        if mark.kind == Kind::Text || mark.kind == Kind::Step {
            let tag = if mark.kind == Kind::Text {
                format!(
                    "<text x=\"{}\" y=\"{}\" font-size=\"{}\" font-family=\"{}\" fill=\"#{:02x}{:02x}{:02x}\">{}</text>",
                    p.x,
                    p.y + mark.width,
                    mark.width,
                    xml(&mark.font),
                    mark.color[0],
                    mark.color[1],
                    mark.color[2],
                    xml(&mark.text)
                )
            } else {
                format!(
                    "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-family=\"sans-serif\" font-weight=\"bold\" font-size=\"{}\" fill=\"white\">{}</text>",
                    p.x,
                    p.y + mark.width,
                    mark.width * 3.5,
                    mark.number
                )
            };
            let svg = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"{x} {y} {w} {h}\">{tag}</svg>"
            );
            let text = decode_svg(svg.as_bytes(), w, h)?;
            let text = RgbaImage::from_raw(w, h, text.rgba).ok_or(ImageError::InvalidSize)?;
            image::imageops::overlay(&mut overlay, &text, 0, 0);
        }
        image::imageops::overlay(&mut base, &overlay, i64::from(x), i64::from(y));
    }
    Ok(DynamicImage::ImageRgba8(base))
}
