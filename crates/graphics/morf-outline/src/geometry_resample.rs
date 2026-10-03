use super::geometry::{Cubic, MAX_CURVES, point};
use std::f64::consts::PI;
fn length(c: &Cubic) -> f64 {
    let (mut total, mut p) = (0.0, (c[0], c[1]));
    for i in 1..=8 {
        let q = point(c, i as f64 / 8.0);
        total += (q.0 - p.0).hypot(q.1 - p.1);
        p = q;
    }
    total
}
fn split(c: &Cubic, t: f64) -> (Cubic, Cubic) {
    let lerp = |a, b| a + (b - a) * t;
    let (x01, y01, x12, y12, x23, y23) = (
        lerp(c[0], c[2]),
        lerp(c[1], c[3]),
        lerp(c[2], c[4]),
        lerp(c[3], c[5]),
        lerp(c[4], c[6]),
        lerp(c[5], c[7]),
    );
    let (xa, ya, xb, yb) = (
        lerp(x01, x12),
        lerp(y01, y12),
        lerp(x12, x23),
        lerp(y12, y23),
    );
    let (xm, ym) = (lerp(xa, xb), lerp(ya, yb));
    (
        [c[0], c[1], x01, y01, xa, ya, xm, ym],
        [xm, ym, xb, yb, x23, y23, c[6], c[7]],
    )
}
pub(super) fn resample(curves: &[Cubic], count: usize) -> Result<Vec<Cubic>, String> {
    let lengths: Vec<_> = curves.iter().map(length).collect();
    let total: f64 = lengths.iter().sum();
    let live = lengths.iter().filter(|l| **l > 1e-6).count();
    if live == 0 || count < live || count > MAX_CURVES {
        return Err("too few or too many segments for this outline".into());
    }
    let mut shares = vec![0; curves.len()];
    let (mut given, mut order) = (0, Vec::new());
    for (i, l) in lengths.iter().enumerate() {
        if *l > 1e-6 {
            let exact = 1.0 + (count - live) as f64 * l / total;
            shares[i] = exact.floor() as usize;
            given += shares[i];
            order.push((i, exact - shares[i] as f64));
        }
    }
    order.sort_by(|a, b| b.1.total_cmp(&a.1));
    for k in 0..count.saturating_sub(given) {
        shares[order[k % order.len()].0] += 1;
    }
    let mut out = Vec::with_capacity(count);
    for (c, n) in curves.iter().zip(shares) {
        let mut rest = *c;
        for parts in (2..=n).rev() {
            let (head, next) = split(&rest, 1.0 / parts as f64);
            out.push(head);
            rest = next;
        }
        if n >= 1 {
            out.push(rest);
        }
    }
    Ok(out)
}
pub(super) fn normalise(mut curves: Vec<Cubic>) -> Result<Vec<Cubic>, String> {
    let (mut minx, mut miny, mut maxx, mut maxy) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for c in &curves {
        for t in 0..=8 {
            let (x, y) = point(c, t as f64 / 8.0);
            minx = minx.min(x);
            maxx = maxx.max(x);
            miny = miny.min(y);
            maxy = maxy.max(y);
        }
    }
    let span = (maxx - minx).max(maxy - miny);
    if span < 1e-12 {
        return Err("outline has no extent".into());
    }
    let scale = 1.0 / span;
    let (cx, cy) = ((minx + maxx) / 2.0, (miny + maxy) / 2.0);
    let (mut best, mut first) = (f64::INFINITY, 0);
    for (i, c) in curves.iter_mut().enumerate() {
        for k in (0..8).step_by(2) {
            c[k] = (c[k] - cx) * scale + 0.5;
            c[k + 1] = (c[k + 1] - cy) * scale + 0.5;
        }
        let a = (c[1] - 0.5).atan2(c[0] - 0.5) + PI / 2.0;
        let off = ((a + PI).rem_euclid(2.0 * PI) - PI).abs();
        if off < best - 1e-6 {
            best = off;
            first = i;
        }
    }
    curves.rotate_left(first);
    Ok(curves)
}
