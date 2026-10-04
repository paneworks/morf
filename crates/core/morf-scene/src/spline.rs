//! Multi-segment Bézier timing curves.
//!
//! A spline is a run of cubic segments laid end to end across the unit
//! interval: it starts at `(0, 0)`, each segment is two control points and an
//! end point, and the last segment ends at `(1, 1)`. It is the shape Qt's
//! `Easing.BezierSpline` takes, and the reason for it is what one cubic cannot
//! say: an ease that overshoots and comes back, a pause part way, a settle in
//! two steps. `y` is free to leave `[0, 1]` — that is the overshoot — while
//! `x` is time and has to keep moving forwards.
//!
//! The curves are interned. An [`Easing`](crate::Easing) is `Copy` and is
//! compared every time a behavior is installed, so a spline is held as a
//! `&'static [f64]` shared by every behavior that names the same points,
//! rather than as a vector each of them owns. A configuration names a handful
//! of curves; the table keeps one copy of each however often they are named.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// How many distinct curves one process may intern.
///
/// A curve lives for the rest of the process once named, so a configuration
/// that built a fresh one every frame would grow without bound. Real ones name
/// a few; the cap turns the runaway into an error at the line that caused it.
pub const MAX_SPLINES: usize = 4096;

/// How many segments one curve may have.
pub const MAX_SPLINE_SEGMENTS: usize = 64;

/// Every curve interned so far, keyed by the bits of its points.
type SplineTable = HashMap<Vec<u64>, &'static [f64]>;

fn table() -> &'static Mutex<SplineTable> {
    static TABLE: OnceLock<Mutex<SplineTable>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Checks a flat list of spline points and interns it.
///
/// `points` is `x1, y1, x2, y2, x, y` per segment: the two control points and
/// where the segment ends. Every number must be finite, the curve must end at
/// `(1, 1)`, each segment's end must not come before the one before it, and
/// its control points' `x` must lie between the two ends — which keeps `x`
/// inside the segment, so time never runs backwards.
pub fn intern_spline(points: &[f64]) -> Result<&'static [f64], String> {
    if points.is_empty() || !points.len().is_multiple_of(6) {
        return Err("easing spline must be groups of six numbers: x1, y1, x2, y2, x, y".into());
    }
    if points.len() / 6 > MAX_SPLINE_SEGMENTS {
        return Err(format!(
            "easing spline has more than {MAX_SPLINE_SEGMENTS} segments"
        ));
    }
    if points.iter().any(|value| !value.is_finite()) {
        return Err("easing spline points must be finite numbers".into());
    }
    let mut start_x = 0.0;
    for segment in points.as_chunks::<6>().0 {
        let [x1, _, x2, _, x, _] = [
            segment[0], segment[1], segment[2], segment[3], segment[4], segment[5],
        ];
        if x < start_x {
            return Err("easing spline segments must move forwards in x".into());
        }
        if !(start_x..=x).contains(&x1) || !(start_x..=x).contains(&x2) {
            return Err(
                "easing spline control points must lie between their segment's ends in x".into(),
            );
        }
        start_x = x;
    }
    let last = &points[points.len() - 6..];
    if (last[4] - 1.0).abs() > 1e-9 || (last[5] - 1.0).abs() > 1e-9 {
        return Err("easing spline must end at (1, 1)".into());
    }
    // Keyed by the bits, so two lists that print the same are one curve and
    // `-0.0` is not mistaken for `0.0`'s twin by a float comparison.
    let bits: Vec<u64> = points.iter().map(|value| value.to_bits()).collect();
    let mut table = table()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(found) = table.get(&bits) {
        return Ok(found);
    }
    if table.len() >= MAX_SPLINES {
        return Err(format!(
            "more than {MAX_SPLINES} distinct easing splines; build a curve once and reuse it"
        ));
    }
    let leaked: &'static [f64] = Box::leak(points.to_vec().into_boxed_slice());
    table.insert(bits, leaked);
    Ok(leaked)
}

/// One coordinate of a cubic Bézier through `p0 … p3` at `t`.
fn cubic(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
}

/// Its derivative in `t`.
fn cubic_slope(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    3.0 * u * u * (p1 - p0) + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (p3 - p2)
}

/// The curve's `y` where its `x` is `progress`.
///
/// Finds the segment `progress` falls in, then the parameter along it whose
/// `x` is `progress`: Newton's method from a linear guess, which lands in two
/// or three steps on any sensible curve, falling back to bisection — always
/// convergent, because the control points keep `x` inside the segment — when
/// the slope is too flat to divide by or a step leaves the bracket.
pub fn spline_value(points: &[f64], progress: f64) -> f64 {
    let x = progress.clamp(0.0, 1.0);
    let (mut start_x, mut start_y) = (0.0, 0.0);
    for segment in points.as_chunks::<6>().0 {
        let (x1, y1, x2, y2, end_x, end_y) = (
            segment[0], segment[1], segment[2], segment[3], segment[4], segment[5],
        );
        if x <= end_x || end_x >= 1.0 {
            let span = end_x - start_x;
            if span <= 1e-12 {
                return end_y;
            }
            let t = solve_segment(start_x, x1, x2, end_x, x, span);
            return cubic(start_y, y1, y2, end_y, t);
        }
        start_x = end_x;
        start_y = end_y;
    }
    // A curve ends at (1, 1), and 1 is as far as `x` goes.
    1.0
}

fn solve_segment(x0: f64, x1: f64, x2: f64, x3: f64, target: f64, span: f64) -> f64 {
    let (mut low, mut high) = (0.0_f64, 1.0_f64);
    let mut t = ((target - x0) / span).clamp(0.0, 1.0);
    for _ in 0..32 {
        let error = cubic(x0, x1, x2, x3, t) - target;
        if error.abs() < 1e-10 {
            return t;
        }
        if error > 0.0 {
            high = t;
        } else {
            low = t;
        }
        let slope = cubic_slope(x0, x1, x2, x3, t);
        let newton = t - error / slope;
        t = if slope.abs() > 1e-9 && newton > low && newton < high {
            newton
        } else {
            (low + high) / 2.0
        };
    }
    t
}
