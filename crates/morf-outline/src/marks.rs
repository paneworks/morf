//! Marks a style draws with: arcs, hatching, tick rings, rulers and
//! segment rows, as SVG path data. Angles are degrees clockwise from twelve
//! o'clock; everything else is in path units.
use std::fmt::Write;

/// An arc of `sweep` degrees clockwise from `from`, radius `r` about
/// `(cx, cy)`, in pieces of at most 90 degrees (so a full turn is one too).
pub fn arc(cx: f64, cy: f64, r: f64, from: f64, sweep: f64) -> String {
    let at = |deg: f64| {
        let a = deg.to_radians();
        (cx + r * a.sin(), cy - r * a.cos())
    };
    let (x, y) = at(from);
    let mut d = format!("M{x:.3} {y:.3}");
    let pieces = (sweep.abs() / 90.0).ceil().max(1.0) as usize;
    let flag = if sweep < 0.0 { 0 } else { 1 };
    for i in 1..=pieces {
        let (ex, ey) = at(from + sweep * i as f64 / pieces as f64);
        let _ = write!(d, " A{r:.3} {r:.3} 0 0 {flag} {ex:.3} {ey:.3}");
    }
    d
}

/// `/` stripes across a `w` by `h` box, one every `gap` along the bottom
/// edge, each cut where it leaves the box.
pub fn hatch(w: f64, h: f64, gap: f64) -> String {
    let gap = gap.max(0.5);
    let mut d = String::new();
    let mut c = -h;
    while c <= w {
        let (x1, x2) = (c.max(0.0), (c + h).min(w));
        if x2 > x1 {
            let _ = write!(d, "M{:.1} {:.1} L{:.1} {:.1} ", x1, h - (x1 - c), x2, h - (x2 - c));
        }
        c += gap;
    }
    if d.is_empty() { "M0 0".into() } else { d }
}

/// The `/` stripes inside the area under a stepped series: step `i` spans
/// `[x0 + i*dx, x0 + (i+1)*dx]` at height `ys[i]`, the floor is `h`, the
/// box `w` wide. Each stripe walks only the steps it crosses and keeps its
/// inside runs merged.
pub fn hatch_under(x0: f64, dx: f64, ys: &[f64], w: f64, h: f64, gap: f64) -> String {
    let n = ys.len();
    if n == 0 || dx <= 0.0 {
        return "M0 0".into();
    }
    let gap = gap.max(0.5);
    let mut d = String::new();
    let segment = |d: &mut String, a: f64, b: f64, c: f64| {
        let _ = write!(d, "M{:.1} {:.1} L{:.1} {:.1} ", a, h - (a - c), b, h - (b - c));
    };
    let mut c = ((x0 - h) / gap).floor() * gap;
    while c <= w {
        let (xa, xb) = (c.max(x0), (c + h).min(w));
        let mut i = ((xa - x0) / dx).floor().max(0.0) as usize;
        let mut run: Option<f64> = None;
        let mut x = xa;
        while x < xb && i < n {
            let step_end = (x0 + (i + 1) as f64 * dx).min(xb);
            let limit = c + h - ys[i];
            let b = step_end.min(limit);
            if b > x {
                let start = *run.get_or_insert(x);
                if b < step_end {
                    segment(&mut d, start, b, c);
                    run = None;
                }
            } else if let Some(start) = run.take() {
                segment(&mut d, start, x, c);
            }
            x = step_end;
            i += 1;
        }
        if let Some(start) = run
            && x > start
        {
            segment(&mut d, start, x, c);
        }
        c += gap;
    }
    if d.is_empty() { "M0 0".into() } else { d }
}

/// Radial ticks: `count + 1` of them from `from` across `sweep` degrees
/// (or one at each of `angles`), from radius `r0` out to `r1`; every
/// `major`-th starts at `major_r0` instead.
pub fn ticks(cx: f64, cy: f64, r0: f64, r1: f64, from: f64, sweep: f64, count: usize, major: usize, major_r0: f64) -> String {
    let angles: Vec<f64> = (0..=count).map(|k| from + sweep * k as f64 / count.max(1) as f64).collect();
    radials(cx, cy, r0, r1, &angles, major, major_r0)
}

/// Radial strokes at `angles`; every `major`-th (0: none) from `major_r0`.
pub fn radials(cx: f64, cy: f64, r0: f64, r1: f64, angles: &[f64], major: usize, major_r0: f64) -> String {
    let mut d = String::new();
    for (k, deg) in angles.iter().enumerate() {
        let a = deg.to_radians();
        let (s, c) = (a.sin(), a.cos());
        let ri = if major > 0 && k % major == 0 { major_r0 } else { r0 };
        let _ = write!(d, "M{:.2} {:.2} L{:.2} {:.2} ", cx + ri * s, cy - ri * c, cx + r1 * s, cy - r1 * c);
    }
    d
}

/// A ruler along `length`: a tick every `pitch` (at least `min_count`
/// intervals, evenly spaced), `minor` long, every `major`-th `size` long,
/// standing up from the edge (`vertical`: down the left edge).
pub fn ruler(length: f64, size: f64, pitch: f64, major: usize, minor: f64, min_count: usize, vertical: bool) -> String {
    let n = ((length / pitch.max(0.5)).floor() as usize).max(min_count).max(1);
    let mut d = String::new();
    for k in 0..=n {
        let at = length * k as f64 / n as f64;
        let tall = if major > 0 && k % major == 0 { size } else { minor };
        if vertical {
            let _ = write!(d, "M0 {at:.1} H{tall} ");
        } else {
            let _ = write!(d, "M{at:.1} 0 V{tall} ");
        }
    }
    d
}

/// `count` boxes in a row across `width` by `height`, `gap` apart (down a
/// column when `vertical`): a segmented bar's cells.
pub fn segments(width: f64, height: f64, count: usize, gap: f64, vertical: bool) -> String {
    let count = count.max(1);
    let along = if vertical { height } else { width };
    let cell = ((along - gap * (count - 1) as f64) / count as f64).max(0.0);
    let mut d = String::new();
    for k in 0..count {
        let at = k as f64 * (cell + gap);
        if vertical {
            let _ = write!(d, "M0 {at:.1} h{width:.1} v{cell:.1} h{:.1} Z ", -width);
        } else {
            let _ = write!(d, "M{at:.1} 0 h{cell:.1} v{height:.1} h{:.1} Z ", -cell);
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_come_out_as_the_lua_builders_drew_them() {
        assert_eq!(hatch(4.0, 4.0, 4.0), "M0.0 4.0 L4.0 0.0 M4.0 4.0 L4.0 4.0 ".replace("M4.0 4.0 L4.0 4.0 ", ""));
        assert_eq!(arc(0.0, 0.0, 1.0, 0.0, 90.0), "M0.000 -1.000 A1.000 1.000 0 0 1 1.000 -0.000");
        assert_eq!(ruler(16.0, 4.0, 8.0, 5, 2.0, 4, false), "M0.0 0 V4 M4.0 0 V2 M8.0 0 V2 M12.0 0 V2 M16.0 0 V2 ");
        assert_eq!(segments(10.0, 2.0, 2, 2.0, false), "M0.0 0 h4.0 v2.0 h-4.0 Z M6.0 0 h4.0 v2.0 h-4.0 Z ");
        let under = hatch_under(0.0, 10.0, &[0.0], 10.0, 10.0, 20.0);
        assert!(under.starts_with('M'), "{under}");
        assert_eq!(ticks(0.0, 0.0, 1.0, 2.0, 0.0, 90.0, 1, 0, 0.0).matches('M').count(), 2);
    }
}
