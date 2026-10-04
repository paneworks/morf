//! The rest of the plot kinds: histograms, radial and polar bars, state
//! runs and cells, waveforms and x-y lines.
use std::fmt::Write;

use super::*;
use crate::marks;
use crate::series::Plot;

/// The counts of `values` in `bins` equal bins across `left..right`
/// (their range when not given).
pub fn histogram_counts(values: &[f32], plot: &Plot) -> Vec<f32> {
    let bins = plot.bins.max(1);
    let (left, right) = range(values.iter().map(|v| *v as f64), plot.left, plot.right);
    let mut counts = vec![0f32; bins];
    for v in values {
        let at = ((*v as f64 - left) / (right - left) * bins as f64).floor();
        if at >= 0.0 && (at as usize) <= bins {
            counts[(at as usize).min(bins - 1)] += 1.0;
        }
    }
    counts
}

/// Radial bars: each value a band of its own ring, from `inner` (a
/// fraction of the radius) out, swept `sweep` degrees from `start` in
/// proportion to its value.
pub fn radial(values: &[f32], plot: &Plot, top: f64) -> String {
    let n = values.len();
    if n == 0 {
        return "M0 0".into();
    }
    let (cx, cy) = (plot.width / 2.0, plot.height / 2.0);
    let outer = cx.min(cy);
    let inner = outer * plot.inner.clamp(0.0, 0.95);
    let gap = plot.gap.max(0.0);
    let band = ((outer - inner) - gap * (n - 1) as f64) / n as f64;
    let span = (top - plot.bottom).max(1e-9);
    let mut d = String::new();
    for (k, v) in values.iter().enumerate() {
        let r1 = outer - k as f64 * (band + gap);
        let r0 = (r1 - band).max(0.0);
        let sweep = plot.sweep * ((*v as f64 - plot.bottom) / span).clamp(0.0, 1.0);
        if sweep > 0.01 {
            if plot.arcs {
                // The band's middle as an open arc: stroked `band` wide, its
                // ends round as the stroke's cap says.
                d.push_str(&marks::arc(cx, cy, (r0 + r1) / 2.0, plot.start, sweep));
            } else {
                d.push_str(&marks::sector(cx, cy, r0, r1, plot.start, sweep));
            }
            d.push(' ');
        }
    }
    if d.is_empty() { "M0 0".into() } else { d }
}

/// The runs of samples in state `state` (a value rounded), as full-height
/// bars, right-aligned like a line: one state's colour of a timeline.
pub fn states(values: &[f32], plot: &Plot) -> String {
    let count = if plot.samples == 0 {
        values.len().max(1)
    } else {
        plot.samples.max(1)
    };
    let shown = &values[values.len().saturating_sub(count)..];
    let dx = plot.width / count as f64;
    let x0 = plot.width - shown.len() as f64 * dx;
    let mut d = String::new();
    let mut run: Option<usize> = None;
    let want = plot.state.round();
    for i in 0..=shown.len() {
        let on = i < shown.len() && (shown[i] as f64).round() == want;
        match (on, run) {
            (true, None) => run = Some(i),
            (false, Some(start)) => {
                let x = x0 + start as f64 * dx;
                rect(
                    &mut d,
                    x,
                    0.0,
                    (i - start) as f64 * dx - plot.gap.max(0.0).min(dx / 2.0),
                    plot.height,
                    plot.radius,
                );
                run = None;
            }
            _ => {}
        }
    }
    if d.is_empty() { "M0 0".into() } else { d }
}

/// Status history: a square per sample in state `state`, in a row (or
/// `rows` to a column), right-aligned.
pub fn state_cells(values: &[f32], plot: &Plot) -> String {
    let rows = plot.rows.max(1);
    let want = plot.state.round();
    let mask: Vec<f32> = values
        .iter()
        .map(|v| {
            if (*v as f64).round() == want {
                1.0
            } else {
                0.0
            }
        })
        .collect();
    let cells_plot = Plot {
        bottom: 0.0,
        lo: 0.5,
        hi: 1.0,
        rows,
        ..*plot
    };
    cells(&mask, &cells_plot, 1.0)
}

/// A waveform: each sample's amplitude drawn up and down from the middle,
/// right-aligned like a line, as one filled shape.
pub fn wave(values: &[f32], plot: &Plot, top: f64) -> String {
    let count = if plot.samples == 0 {
        values.len().max(2)
    } else {
        plot.samples.max(2)
    };
    let shown = &values[values.len().saturating_sub(count)..];
    if shown.is_empty() {
        return "M0 0".into();
    }
    let dx = plot.width / (count - 1) as f64;
    let x0 = plot.width - shown.len().saturating_sub(1) as f64 * dx;
    let mid = plot.height / 2.0;
    let span = (top - plot.bottom).max(1e-9);
    let a = |v: f32| {
        (((v as f64).abs() - plot.bottom) / span).clamp(0.0, 1.0) * (mid - plot.pad_top).max(0.0)
    };
    let mut d = String::new();
    for (i, v) in shown.iter().enumerate() {
        let _ = write!(
            d,
            "{}{:.2} {:.2} ",
            if i == 0 { 'M' } else { 'L' },
            x0 + i as f64 * dx,
            mid - a(*v).max(0.5)
        );
    }
    for (i, v) in shown.iter().enumerate().rev() {
        let _ = write!(d, "L{:.2} {:.2} ", x0 + i as f64 * dx, mid + a(*v).max(0.5));
    }
    d.push('Z');
    d
}

/// Bars standing out from a ring: one per value, evenly round the circle
/// from `start` across `sweep` degrees, rising from `inner` (a fraction of
/// the radius) by its value; `gap` is the share of each slot left empty.
pub fn polar_bars(values: &[f32], plot: &Plot, top: f64) -> String {
    let n = values.len();
    if n == 0 {
        return "M0 0".into();
    }
    let (cx, cy) = (plot.width / 2.0, plot.height / 2.0);
    let outer = cx.min(cy);
    let inner = outer * plot.inner.clamp(0.0, 0.95);
    let span = (top - plot.bottom).max(1e-9);
    let slot = plot.sweep / n as f64;
    let fill = slot * (1.0 - (plot.gap / 10.0).clamp(0.0, 0.9));
    let mut d = String::new();
    for (k, v) in values.iter().enumerate() {
        let level = ((*v as f64 - plot.bottom) / span).clamp(0.0, 1.0);
        let r1 = inner + (outer - inner) * level.max(0.02);
        let from = plot.start + k as f64 * slot + (slot - fill) / 2.0;
        d.push_str(&marks::sector(cx, cy, inner, r1, from, fill));
        d.push(' ');
    }
    d
}

/// A line through `x, y` pairs in order: a Lissajous figure, a phase plot.
/// `x` across `left..right` (their range when not given), `y` up
/// `bottom..top`.
pub fn xy_line(values: &[f32], plot: &Plot, top: f64) -> String {
    let pairs: Vec<(f64, f64)> = values
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| (p[0] as f64, p[1] as f64))
        .collect();
    if pairs.len() < 2 {
        return "M0 0".into();
    }
    let (left, right) = range(pairs.iter().map(|p| p.0), plot.left, plot.right);
    let span = (top - plot.bottom).max(1e-9);
    let mut d = String::with_capacity(pairs.len() * 16);
    for (i, (x, y)) in pairs.iter().enumerate() {
        let px = (x - left) / (right - left) * plot.width;
        let py = plot.height - ((y - plot.bottom) / span).clamp(0.0, 1.0) * plot.height;
        let _ = write!(d, "{}{px:.1} {py:.1} ", if i == 0 { 'M' } else { 'L' });
    }
    d
}
