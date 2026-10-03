//! The plot kinds beyond a single line: cells (heatmaps, spectrograms,
//! calendars), scatter, candles, boxes, stacked layers and bars,
//! histograms, radial bars, state runs and cells, and waveforms. Each reads
//! a channel's flat run of numbers in a layout of its own, said on each.
use std::fmt::Write;

use crate::marks;
use crate::series::Plot;

fn rect(d: &mut String, x: f64, y: f64, w: f64, h: f64, r: f64) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    if r <= 0.05 {
        let _ = write!(d, "M{x:.1} {y:.1} h{w:.1} v{h:.1} h{:.1} Z ", -w);
    } else {
        let _ = write!(
            d,
            "M{:.1} {y:.1} H{:.1} A{r:.1} {r:.1} 0 0 1 {:.1} {:.1} V{:.1} A{r:.1} {r:.1} 0 0 1 {:.1} {:.1} H{:.1} A{r:.1} {r:.1} 0 0 1 {x:.1} {:.1} V{:.1} A{r:.1} {r:.1} 0 0 1 {:.1} {y:.1} Z ",
            x + r,
            x + w - r,
            x + w,
            y + r,
            y + h - r,
            x + w - r,
            y + h,
            x + r,
            y + h - r,
            y + r,
            x + r
        );
    }
}

fn circle(d: &mut String, cx: f64, cy: f64, r: f64) {
    let _ = write!(d, "M{:.1} {cy:.1} a{r:.1} {r:.1} 0 1 0 {:.1} 0 a{r:.1} {r:.1} 0 1 0 {:.1} 0 Z ", cx - r, 2.0 * r, -2.0 * r);
}

fn range(values: impl Iterator<Item = f64> + Clone, low: Option<f64>, high: Option<f64>) -> (f64, f64) {
    let lo = low.unwrap_or_else(|| values.clone().fold(f64::INFINITY, f64::min));
    let hi = high.unwrap_or_else(|| values.fold(f64::NEG_INFINITY, f64::max));
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 1.0);
    }
    if hi - lo < 1e-9 { (lo, lo + 1.0) } else { (lo, hi) }
}

/// Cells, column by column (`rows` to a column, the newest column at the
/// right when there are more than fit): those whose place between `bottom`
/// and `top` falls in `[lo, hi)` -- one tone of a heatmap, drawn by one path.
pub fn cells(values: &[f32], plot: &Plot, top: f64) -> String {
    let rows = plot.rows.max(1);
    let total_cols = values.len().div_ceil(rows).max(1);
    let cols = if plot.columns > 0 { plot.columns } else { total_cols };
    let gap = plot.gap.max(0.0);
    let cw = (plot.width - gap * (cols - 1) as f64) / cols as f64;
    let ch = (plot.height - gap * (rows - 1) as f64) / rows as f64;
    let span = (top - plot.bottom).max(1e-9);
    let skip = total_cols.saturating_sub(cols);
    let offset = cols.saturating_sub(total_cols);
    let mut d = String::new();
    for (i, v) in values.iter().enumerate().skip(skip * rows) {
        let n = ((*v as f64 - plot.bottom) / span).clamp(0.0, 1.0);
        let inside = n >= plot.lo && (n < plot.hi || (plot.hi >= 1.0 && n <= 1.0));
        if !inside {
            continue;
        }
        let col = i / rows - skip + offset;
        let row = i % rows;
        rect(&mut d, col as f64 * (cw + gap), row as f64 * (ch + gap), cw, ch, plot.radius);
    }
    if d.is_empty() { "M0 0".into() } else { d }
}

/// Points from `x, y` pairs: `x` across `left..right` (their range when not
/// given), `y` up `bottom..top`; a dot of radius `point` each.
pub fn scatter(values: &[f32], plot: &Plot, top: f64) -> String {
    let pairs: Vec<(f64, f64)> = values.chunks_exact(2).map(|p| (p[0] as f64, p[1] as f64)).collect();
    let (left, right) = range(pairs.iter().map(|p| p.0), plot.left, plot.right);
    let span = (top - plot.bottom).max(1e-9);
    let r = plot.point.max(0.5);
    let mut d = String::new();
    for (x, y) in pairs {
        let px = r + (x - left) / (right - left) * (plot.width - 2.0 * r);
        let py = plot.height - r - ((y - plot.bottom) / span).clamp(0.0, 1.0) * (plot.height - 2.0 * r);
        circle(&mut d, px, py, r);
    }
    if d.is_empty() { "M0 0".into() } else { d }
}

/// Candles from `open, high, low, close` fours: the body between open and
/// close, a wick from low to high; `direction` 1 draws only rising ones, -1
/// only falling ones, 0 all.
pub fn candles(values: &[f32], plot: &Plot, top: f64) -> String {
    let quads: Vec<[f64; 4]> = values.chunks_exact(4).map(|q| [q[0] as f64, q[1] as f64, q[2] as f64, q[3] as f64]).collect();
    let n = quads.len();
    if n == 0 {
        return "M0 0".into();
    }
    let (low, high) = range(quads.iter().flat_map(|q| [q[1], q[2]]), Some(plot.bottom).filter(|_| plot.top.is_some()), plot.top.map(|_| top));
    let y = |v: f64| plot.height - (v - low) / (high - low) * plot.height;
    let gap = plot.gap.max(0.0);
    let bw = ((plot.width - gap * (n - 1) as f64) / n as f64).max(1.0);
    let mut d = String::new();
    for (k, [open, hi, lo, close]) in quads.into_iter().enumerate() {
        let rising = close >= open;
        if (plot.direction > 0 && !rising) || (plot.direction < 0 && rising) {
            continue;
        }
        let x = k as f64 * (bw + gap);
        let (a, b) = (y(open.max(close)), y(open.min(close)));
        rect(&mut d, x, a, bw, (b - a).max(1.0), plot.radius);
        let wick = 1.0f64.max(bw / 8.0);
        rect(&mut d, x + (bw - wick) / 2.0, y(hi), wick, (y(lo) - y(hi)).max(0.0), 0.0);
    }
    if d.is_empty() { "M0 0".into() } else { d }
}

/// Box plots from `min, q1, median, q3, max` fives: the box from q1 to q3
/// with a gap at the median, whiskers to min and max, and their caps.
pub fn boxes(values: &[f32], plot: &Plot, top: f64) -> String {
    let fives: Vec<[f64; 5]> = values.chunks_exact(5).map(|f| [f[0] as f64, f[1] as f64, f[2] as f64, f[3] as f64, f[4] as f64]).collect();
    let n = fives.len();
    if n == 0 {
        return "M0 0".into();
    }
    let span = (top - plot.bottom).max(1e-9);
    let y = |v: f64| plot.height - ((v - plot.bottom) / span).clamp(0.0, 1.0) * plot.height;
    let gap = plot.gap.max(0.0);
    let bw = ((plot.width - gap * (n - 1) as f64) / n as f64).max(3.0);
    let line = 1.5f64;
    let mut d = String::new();
    for (k, [min, q1, median, q3, max]) in fives.into_iter().enumerate() {
        let x = k as f64 * (bw + gap);
        let mid = x + bw / 2.0;
        rect(&mut d, x, y(q3), bw, (y(median) - y(q3) - line / 2.0).max(0.0), plot.radius);
        rect(&mut d, x, y(median) + line / 2.0, bw, (y(q1) - y(median) - line / 2.0).max(0.0), plot.radius);
        rect(&mut d, mid - line / 2.0, y(max), line, (y(q3) - y(max)).max(0.0), 0.0);
        rect(&mut d, mid - line / 2.0, y(q1), line, (y(min) - y(q1)).max(0.0), 0.0);
        rect(&mut d, x + bw / 4.0, y(max) - line / 2.0, bw / 2.0, line, 0.0);
        rect(&mut d, x + bw / 4.0, y(min) - line / 2.0, bw / 2.0, line, 0.0);
    }
    d
}

/// The cumulative sums of `layers` interleaved series up to layer `k`.
fn stacked(values: &[f32], layers: usize, k: usize) -> Vec<(f64, f64)> {
    values
        .chunks_exact(layers)
        .map(|sample| {
            let below: f64 = sample[..k].iter().map(|v| v.max(0.0) as f64).sum();
            (below, below + sample[k].max(0.0) as f64)
        })
        .collect()
}

/// The top a stack scales to: the highest cumulative sample, with headroom.
pub fn stack_top(values: &[f32], plot: &Plot) -> f64 {
    if let Some(top) = plot.top {
        return top;
    }
    let layers = plot.layers.max(1);
    let peak = values
        .chunks_exact(layers)
        .map(|s| s.iter().map(|v| v.max(0.0) as f64).sum::<f64>())
        .fold(plot.bottom, f64::max);
    (peak * plot.headroom).max(plot.bottom + plot.floor)
}

/// Layer `layer` of `layers` interleaved series (sample by sample), stacked
/// on those before it: an area between its floor and its top, right-aligned
/// like a line; `bars` draws it as a bar per sample instead.
pub fn stack(values: &[f32], plot: &Plot, bars: bool) -> String {
    let layers = plot.layers.max(1);
    let k = plot.layer.min(layers - 1);
    let top = stack_top(values, plot);
    let span = (top - plot.bottom).max(1e-9);
    let y = |v: f64| plot.height - plot.pad_bottom - ((v - plot.bottom) / span).clamp(0.0, 1.0) * (plot.height - plot.pad_bottom - plot.pad_top);
    let levels = stacked(values, layers, k);
    let n = levels.len();
    if n == 0 {
        return "M0 0".into();
    }
    let mut d = String::new();
    if bars {
        let gap = plot.gap.max(0.0);
        let bw = ((plot.width - gap * (n - 1) as f64) / n as f64).max(1.0);
        for (i, (lo, hi)) in levels.iter().enumerate() {
            let (a, b) = (y(*hi), y(*lo));
            rect(&mut d, i as f64 * (bw + gap), a, bw, b - a, if k == layers - 1 { plot.radius } else { 0.0 });
        }
        return if d.is_empty() { "M0 0".into() } else { d };
    }
    let count = if plot.samples == 0 { n.max(2) } else { plot.samples.max(2) };
    let shown = &levels[n.saturating_sub(count)..];
    let dx = plot.width / (count - 1) as f64;
    let x0 = plot.width - shown.len().saturating_sub(1) as f64 * dx;
    let tops: Vec<(f64, f64)> = shown.iter().enumerate().map(|(i, (_, hi))| (x0 + i as f64 * dx, y(*hi))).collect();
    let floors: Vec<(f64, f64)> = shown.iter().enumerate().rev().map(|(i, (lo, _))| (x0 + i as f64 * dx, y(*lo))).collect();
    if plot.smooth && tops.len() > 1 {
        // Both edges as curves, the floor walked back the way it came.
        curve(&mut d, &tops, true, plot.pad_top, plot.height - plot.pad_bottom);
        curve(&mut d, &floors, false, plot.pad_top, plot.height - plot.pad_bottom);
    } else {
        for (i, (x, y)) in tops.iter().enumerate() {
            let _ = write!(d, "{}{x:.2} {y:.2} ", if i == 0 { 'M' } else { 'L' });
        }
        for (x, y) in &floors {
            let _ = write!(d, "L{x:.2} {y:.2} ");
        }
    }
    d.push('Z');
    d
}

/// A Catmull-Rom curve through `points` as cubics, begun with a move
/// (`start`) or a line from where the path is; controls held in `lo..hi`.
fn curve(d: &mut String, points: &[(f64, f64)], start: bool, lo: f64, hi: f64) {
    let n = points.len();
    let at = |i: isize| points[i.clamp(0, n as isize - 1) as usize];
    let cy = |y: f64| y.clamp(lo.min(hi), hi.max(lo));
    let _ = write!(d, "{}{:.2} {:.2} ", if start { 'M' } else { 'L' }, points[0].0, points[0].1);
    for i in 0..n as isize - 1 {
        let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        let _ = write!(
            d,
            "C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} ",
            p1.0 + (p2.0 - p0.0) / 6.0,
            cy(p1.1 + (p2.1 - p0.1) / 6.0),
            p2.0 - (p3.0 - p1.0) / 6.0,
            cy(p2.1 - (p3.1 - p1.1) / 6.0),
            p2.0,
            p2.1
        );
    }
}

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
    let count = if plot.samples == 0 { values.len().max(1) } else { plot.samples.max(1) };
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
                rect(&mut d, x, 0.0, (i - start) as f64 * dx - plot.gap.max(0.0).min(dx / 2.0), plot.height, plot.radius);
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
    let mask: Vec<f32> = values.iter().map(|v| if (*v as f64).round() == want { 1.0 } else { 0.0 }).collect();
    let cells_plot = Plot { bottom: 0.0, lo: 0.5, hi: 1.0, rows, ..*plot };
    cells(&mask, &cells_plot, 1.0)
}

/// A waveform: each sample's amplitude drawn up and down from the middle,
/// right-aligned like a line, as one filled shape.
pub fn wave(values: &[f32], plot: &Plot, top: f64) -> String {
    let count = if plot.samples == 0 { values.len().max(2) } else { plot.samples.max(2) };
    let shown = &values[values.len().saturating_sub(count)..];
    if shown.is_empty() {
        return "M0 0".into();
    }
    let dx = plot.width / (count - 1) as f64;
    let x0 = plot.width - shown.len().saturating_sub(1) as f64 * dx;
    let mid = plot.height / 2.0;
    let span = (top - plot.bottom).max(1e-9);
    let a = |v: f32| (((v as f64).abs() - plot.bottom) / span).clamp(0.0, 1.0) * (mid - plot.pad_top).max(0.0);
    let mut d = String::new();
    for (i, v) in shown.iter().enumerate() {
        let _ = write!(d, "{}{:.2} {:.2} ", if i == 0 { 'M' } else { 'L' }, x0 + i as f64 * dx, mid - a(*v).max(0.5));
    }
    for (i, v) in shown.iter().enumerate().rev() {
        let _ = write!(d, "L{:.2} {:.2} ", x0 + i as f64 * dx, mid + a(*v).max(0.5));
    }
    d.push('Z');
    d
}
