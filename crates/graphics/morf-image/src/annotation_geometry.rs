//! Bounds and hit tests shared by annotation editors, independent of the UI.
use crate::annotation::{Annotation, Kind};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Annotation {
    pub fn bounds(&self) -> Bounds {
        let Some(p) = self.points.first() else {
            return Bounds {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            };
        };
        let q = self.points.last().unwrap();
        let (mut x, mut y) = (p.x.min(q.x), p.y.min(q.y));
        let (mut w, mut h) = ((q.x - p.x).abs(), (q.y - p.y).abs());
        match self.kind {
            Kind::Pen => {
                let (mut right, mut bottom) = (x + w, y + h);
                for p in &self.points {
                    x = x.min(p.x);
                    y = y.min(p.y);
                    right = right.max(p.x);
                    bottom = bottom.max(p.y);
                }
                w = right - x;
                h = bottom - y;
            }
            Kind::Text => {
                w = 20.0_f32.max(self.text.len() as f32 * self.width * 0.6);
                h = self.width * 1.3;
            }
            Kind::Step => {
                x = p.x - self.width * 3.0;
                y = p.y - self.width * 3.0;
                w = self.width * 6.0;
                h = w;
            }
            _ => {}
        }
        Bounds { x, y, w, h }
    }
    pub fn hit(&self, x: f32, y: f32, tolerance: f32) -> bool {
        if self.points.is_empty() || !x.is_finite() || !y.is_finite() || !tolerance.is_finite() {
            return false;
        }
        let b = self.bounds();
        let pad = tolerance.max(self.width);
        if x < b.x - pad || y < b.y - pad || x > b.x + b.w + pad || y > b.y + b.h + pad {
            return false;
        }
        match self.kind {
            Kind::Line | Kind::Arrow | Kind::Pen => self.points.windows(2).any(|line| {
                let (p, q) = (line[0], line[1]);
                let (dx, dy) = (q.x - p.x, q.y - p.y);
                let t = (((x - p.x) * dx + (y - p.y) * dy) / (dx * dx + dy * dy).max(1.0))
                    .clamp(0.0, 1.0);
                (x - p.x - t * dx).hypot(y - p.y - t * dy) <= pad
            }),
            Kind::Ellipse => {
                let r = ((x - b.x - b.w / 2.0) / (b.w / 2.0).max(1.0))
                    .hypot((y - b.y - b.h / 2.0) / (b.h / 2.0).max(1.0));
                (self.filled && r <= 1.0) || (r - 1.0).abs() <= pad / (b.w.min(b.h) / 2.0).max(1.0)
            }
            Kind::Rect if !self.filled => {
                [
                    (x - b.x).abs(),
                    (x - b.x - b.w).abs(),
                    (y - b.y).abs(),
                    (y - b.y - b.h).abs(),
                ]
                .into_iter()
                .fold(f32::INFINITY, f32::min)
                    <= pad
            }
            _ => true,
        }
    }
}
