//! Rounded polygons and cubic outlines for Path nodes and morphs.
//! Plain geometry; no theme, window, renderer or compositor integration.
use std::f64::consts::PI;
use std::fmt::Write;

pub type Cubic = [f64; 8];
pub type Vertex = [f64; 3];
pub const SEGMENTS: usize = 72;
pub const MAX_CURVES: usize = 4096;

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub rounding: f64,
    pub inner_rounding: Option<f64>,
    pub rotation: f64,
    pub spread: Option<f64>,
}
fn norm(x: f64, y: f64) -> (f64, f64, f64) {
    let length = x.hypot(y);
    if length < 1e-9 {
        (0.0, 0.0, 0.0)
    } else {
        (x / length, y / length, length)
    }
}
fn line(ax: f64, ay: f64, bx: f64, by: f64) -> Cubic {
    [
        ax,
        ay,
        ax + (bx - ax) / 3.0,
        ay + (by - ay) / 3.0,
        ax + 2.0 * (bx - ax) / 3.0,
        ay + 2.0 * (by - ay) / 3.0,
        bx,
        by,
    ]
}
pub fn polygon(vertices: &[Vertex]) -> Result<Vec<Cubic>, String> {
    let n = vertices.len();
    if !(3..=MAX_CURVES / 2).contains(&n)
        || vertices
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 1e6)
    {
        return Err("polygon needs 3..2048 finite vertices within ±1000000".into());
    }
    let mut corners = Vec::with_capacity(n);
    for i in 0..n {
        let (p, v, q) = (
            vertices[(i + n - 1) % n],
            vertices[i],
            vertices[(i + 1) % n],
        );
        let (e1x, e1y, l1) = norm(p[0] - v[0], p[1] - v[1]);
        let (e2x, e2y, l2) = norm(q[0] - v[0], q[1] - v[1]);
        let theta = (e1x * e2x + e1y * e2y).clamp(-1.0, 1.0).acos();
        let (cut, radius) = if v[2] > 0.0 && theta > 1e-4 && theta < PI - 1e-4 {
            let t = (theta / 2.0).tan();
            let cut = (v[2] / t).min(l1 / 2.0).min(l2 / 2.0);
            (cut, cut * t)
        } else {
            (0.0, 0.0)
        };
        let (ax, ay, bx, by) = (
            v[0] + e1x * cut,
            v[1] + e1y * cut,
            v[0] + e2x * cut,
            v[1] + e2y * cut,
        );
        let h = 4.0 / 3.0 * ((PI - theta) / 4.0).tan() * radius;
        corners.push([
            ax,
            ay,
            ax - e1x * h,
            ay - e1y * h,
            bx - e2x * h,
            by - e2y * h,
            bx,
            by,
        ]);
    }
    let mut curves = Vec::with_capacity(n * 2);
    for i in 0..n {
        let c = corners[i];
        let next = corners[(i + 1) % n];
        curves.push(c);
        curves.push(line(c[6], c[7], next[0], next[1]));
    }
    Ok(curves)
}
fn radial(count: usize, inner: Option<f64>, lobes: bool, o: Options) -> Result<Vec<Cubic>, String> {
    let per = if lobes {
        3
    } else if inner.is_some() {
        2
    } else {
        1
    };
    if count < 3 || count > 2048 / per {
        return Err("radial shape count is out of range".into());
    }
    let rot = o.rotation.to_radians() - PI / 2.0;
    let spread = o.spread.unwrap_or(90.0 / count as f64).to_radians();
    let mut vertices = Vec::with_capacity(count * per);
    for i in 0..count {
        let a = rot + 2.0 * PI * i as f64 / count as f64;
        if lobes {
            vertices.push([(a - spread).cos(), (a - spread).sin(), 10.0]);
            vertices.push([(a + spread).cos(), (a + spread).sin(), 10.0]);
        } else {
            vertices.push([a.cos(), a.sin(), o.rounding]);
        }
        if let Some(inner) = inner {
            let b = a + PI / count as f64;
            vertices.push([
                inner * b.cos(),
                inner * b.sin(),
                o.inner_rounding
                    .unwrap_or(if lobes { 0.05 } else { o.rounding }),
            ]);
        }
    }
    polygon(&vertices)
}
pub fn regular(count: usize, o: Options) -> Result<Vec<Cubic>, String> {
    radial(count, None, false, o)
}
pub fn star(count: usize, inner: f64, o: Options) -> Result<Vec<Cubic>, String> {
    radial(count, Some(inner), false, o)
}
pub fn lobes(count: usize, inner: f64, o: Options) -> Result<Vec<Cubic>, String> {
    radial(count, Some(inner), true, o)
}
pub fn point(c: &Cubic, t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    let weights = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    (
        weights.iter().enumerate().map(|(i, w)| w * c[i * 2]).sum(),
        weights
            .iter()
            .enumerate()
            .map(|(i, w)| w * c[i * 2 + 1])
            .sum(),
    )
}
pub fn curves(curves: &[Cubic], segments: Option<usize>) -> Result<Vec<Cubic>, String> {
    if curves.is_empty()
        || curves.len() > MAX_CURVES
        || curves
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 1e6)
    {
        return Err("invalid cubic outline".into());
    }
    let out = match segments {
        Some(n) => super::geometry_resample::resample(curves, n)?,
        None => curves.to_vec(),
    };
    super::geometry_resample::normalise(out)
}
pub fn path(curves: &[Cubic], size: f64) -> Result<String, String> {
    if !size.is_finite() || size <= 0.0 || size > 1e6 || curves.is_empty() {
        return Err("path size must be finite and positive".into());
    }
    let mut out = format!("M{:.2} {:.2}", curves[0][0] * size, curves[0][1] * size);
    for c in curves {
        let _ = write!(
            out,
            " C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}",
            c[2] * size,
            c[3] * size,
            c[4] * size,
            c[5] * size,
            c[6] * size,
            c[7] * size
        );
    }
    out.push_str(" Z");
    Ok(out)
}
