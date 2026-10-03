//! A run of numbers as a path: a chart's line, area or steps, the hatching
//! under its steps, or a row of bars -- what a `ui.Path` reading a data
//! channel draws, and what `morf.geometry.plot` answers.
//!
//! Samples are right-aligned: the newest sits at the right edge, each older
//! one `width / (samples - 1)` to its left, so a history that is filling up
//! grows in from the right. A value maps to `height - pad_bottom - n *
//! (height - pad_bottom - pad_top)` for its place `n` (0..1) between
//! `bottom` and `top`; with no `top`, the top is the peak times `headroom`,
//! and never below `bottom + floor`.
use std::fmt::Write;

use crate::marks;

/// What is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Line,
    Area,
    Steps,
    StepsArea,
    HatchSteps,
    Bars,
}

impl Kind {
    pub fn parse(name: &str) -> Option<Kind> {
        Some(match name {
            "line" => Kind::Line,
            "area" => Kind::Area,
            "steps" => Kind::Steps,
            "steps_area" => Kind::StepsArea,
            "hatch_steps" => Kind::HatchSteps,
            "bars" => Kind::Bars,
            _ => return None,
        })
    }
}

/// How a run of numbers is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plot {
    pub kind: Kind,
    pub width: f64,
    pub height: f64,
    /// How many samples span the width; 0 for as many as there are.
    pub samples: usize,
    pub bottom: f64,
    /// `None`: the peak times `headroom`, at least `bottom + floor`.
    pub top: Option<f64>,
    pub headroom: f64,
    pub floor: f64,
    pub pad_top: f64,
    pub pad_bottom: f64,
    /// A line or area through the samples as a smooth curve.
    pub smooth: bool,
    /// Bars: the gap between them, the most their ends round, the least a
    /// bar stands (no more than its width), and grown from the middle.
    pub gap: f64,
    pub radius: f64,
    pub min_bar: f64,
    pub mirror: bool,
    /// Hatching: a stripe every this many pixels.
    pub hatch: f64,
}

impl Default for Plot {
    fn default() -> Self {
        Plot {
            kind: Kind::Line,
            width: 100.0,
            height: 100.0,
            samples: 0,
            bottom: 0.0,
            top: Some(1.0),
            headroom: 1.0,
            floor: 1.0,
            pad_top: 1.0,
            pad_bottom: 1.0,
            smooth: false,
            gap: 2.0,
            radius: 0.0,
            min_bar: 1.0,
            mirror: false,
            hatch: 6.0,
        }
    }
}

impl Plot {
    /// The top values map to: the given one, or the peak of `values` (and of
    /// `others`, drawn on the same scale) with headroom.
    pub fn top_for(&self, values: &[f32], others: &[f32]) -> f64 {
        if let Some(top) = self.top {
            return top.max(self.bottom + 1e-9);
        }
        let peak = values.iter().chain(others).map(|v| *v as f64).fold(self.bottom, f64::max);
        (peak * self.headroom).max(self.bottom + self.floor)
    }
}

/// The path for `values` (oldest first) drawn as `plot` says. `others` only
/// share the automatic top.
pub fn path(values: &[f32], others: &[f32], plot: &Plot) -> String {
    let (w, h) = (plot.width, plot.height);
    if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 || w > 1e6 || h > 1e6 {
        return "M0 0".into();
    }
    let top = plot.top_for(values, others);
    let span = (top - plot.bottom).max(1e-9);
    let norm = |v: f32| ((v as f64 - plot.bottom) / span).clamp(0.0, 1.0);
    if plot.kind == Kind::Bars {
        return bars(values, plot, &norm);
    }
    let y = |v: f32| h - plot.pad_bottom - norm(v) * (h - plot.pad_bottom - plot.pad_top);
    let count = if plot.samples == 0 { values.len().max(2) } else { plot.samples.max(2) };
    let shown = &values[values.len().saturating_sub(count)..];
    let dx = w / (count - 1) as f64;
    let n = shown.len();
    let x0 = w - n.saturating_sub(1) as f64 * dx;
    let points: Vec<(f64, f64)> = shown.iter().enumerate().map(|(i, v)| (x0 + i as f64 * dx, y(*v))).collect();
    let floor_y = h - plot.pad_bottom;
    let mut d = String::with_capacity(points.len() * 28 + 32);
    match plot.kind {
        Kind::Line | Kind::Area => {
            if points.len() < 2 {
                let _ = write!(d, "M0 {floor_y} H{w}");
                if plot.kind == Kind::Area {
                    let _ = write!(d, " V{h} H0 Z");
                }
                return d;
            }
            if plot.smooth {
                smooth(&mut d, &points, plot.pad_top, floor_y);
            } else {
                for (i, (x, y)) in points.iter().enumerate() {
                    let _ = write!(d, "{}{x:.2} {y:.2} ", if i == 0 { 'M' } else { 'L' });
                }
            }
            if plot.kind == Kind::Area {
                let (lx, fx) = (points[points.len() - 1].0, points[0].0);
                let _ = write!(d, " L{lx:.2} {h} L{fx:.2} {h} Z");
            }
        }
        Kind::Steps | Kind::StepsArea => {
            if points.is_empty() {
                return "M0 0".into();
            }
            let closed = plot.kind == Kind::StepsArea;
            if closed {
                let _ = write!(d, "M{:.1} {:.1} V{:.1}", x0, h, points[0].1);
            } else {
                let _ = write!(d, "M{:.1} {:.1}", x0, points[0].1);
            }
            for (x, y) in &points[1..] {
                let _ = write!(d, " H{x:.1} V{y:.1}");
            }
            if closed {
                let _ = write!(d, " V{h:.1} Z");
            }
        }
        Kind::HatchSteps => {
            let ys: Vec<f64> = points.iter().map(|p| p.1).collect();
            return marks::hatch_under(x0, dx, &ys, w, h, plot.hatch);
        }
        Kind::Bars => unreachable!(),
    }
    d
}

/// A smooth curve through `points` (Catmull-Rom as cubic Béziers), its
/// controls kept between `lo` and `hi` so it never swings past the box.
fn smooth(d: &mut String, points: &[(f64, f64)], lo: f64, hi: f64) {
    let n = points.len();
    let at = |i: isize| points[i.clamp(0, n as isize - 1) as usize];
    let cy = |y: f64| y.clamp(lo.min(hi), hi.max(lo));
    let _ = write!(d, "M{:.2} {:.2}", points[0].0, points[0].1);
    for i in 0..n as isize - 1 {
        let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        let _ = write!(
            d,
            " C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}",
            p1.0 + (p2.0 - p0.0) / 6.0,
            cy(p1.1 + (p2.1 - p0.1) / 6.0),
            p2.0 - (p3.0 - p1.0) / 6.0,
            cy(p2.1 - (p3.1 - p1.1) / 6.0),
            p2.0,
            p2.1
        );
    }
}

fn bars(values: &[f32], plot: &Plot, norm: &dyn Fn(f32) -> f64) -> String {
    let (w, h, gap) = (plot.width, plot.height, plot.gap.max(0.0));
    let n = values.len();
    if n == 0 {
        return "M0 0".into();
    }
    let bw = ((w - gap * (n - 1) as f64) / n as f64).max(1.0);
    let mut d = String::with_capacity(n * 72);
    for (k, v) in values.iter().enumerate() {
        let x = k as f64 * (bw + gap);
        let bh = (norm(*v) * h).max(bw.min(plot.min_bar));
        let r = (bw / 2.0).min(bh / 2.0).min(plot.radius.max(0.0));
        let y = if plot.mirror { (h - bh) / 2.0 } else { h - bh };
        if r <= 0.05 {
            let _ = write!(d, "M{x:.1} {y:.1} h{bw:.1} v{bh:.1} h{:.1} Z ", -bw);
        } else if plot.mirror {
            let _ = write!(
                d,
                "M{x:.1} {:.1} A{r:.1} {r:.1} 0 0 1 {:.1} {y:.1} H{:.1} A{r:.1} {r:.1} 0 0 1 {:.1} {:.1} V{:.1} A{r:.1} {r:.1} 0 0 1 {:.1} {:.1} H{:.1} A{r:.1} {r:.1} 0 0 1 {x:.1} {:.1} Z ",
                y + r,
                x + r,
                x + bw - r,
                x + bw,
                y + r,
                y + bh - r,
                x + bw - r,
                y + bh,
                x + r,
                y + bh - r
            );
        } else {
            let _ = write!(
                d,
                "M{x:.1} {h:.1} V{:.1} A{r:.1} {r:.1} 0 0 1 {:.1} {y:.1} H{:.1} A{r:.1} {r:.1} 0 0 1 {:.1} {:.1} V{h:.1} Z ",
                y + r,
                x + r,
                x + bw - r,
                x + bw,
                y + r
            );
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plots_steps_bars_and_lines() {
        let plot = Plot { kind: Kind::Steps, width: 30.0, height: 10.0, samples: 4, pad_top: 2.0, pad_bottom: 0.0, ..Plot::default() };
        assert_eq!(path(&[0.0, 1.0], &[], &plot), "M20.0 10.0 H30.0 V2.0");
        let bars = Plot { kind: Kind::Bars, width: 10.0, height: 10.0, gap: 0.0, ..Plot::default() };
        assert_eq!(path(&[1.0, 0.5], &[], &bars), "M0.0 0.0 h5.0 v10.0 h-5.0 Z M5.0 5.0 h5.0 v5.0 h-5.0 Z ");
        let auto = Plot { top: None, headroom: 2.0, ..Plot::default() };
        assert_eq!(auto.top_for(&[1.0, 3.0], &[4.0]), 8.0);
        let line = Plot { kind: Kind::Area, smooth: true, width: 10.0, height: 10.0, ..Plot::default() };
        let d = path(&[0.0, 1.0, 0.5], &[], &line);
        assert!(d.starts_with("M0.00 9.00 C") && d.ends_with("Z"), "{d}");
    }
}
