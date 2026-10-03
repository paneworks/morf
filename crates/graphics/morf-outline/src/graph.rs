//! Numeric graph data to Path geometry, independent of sampling and styling.
use std::fmt::Write;
pub const MAX_SAMPLES: usize = 8192;
#[derive(Clone, Copy)]
pub struct Series {
    pub width: f64,
    pub height: f64,
    pub samples: usize,
    pub bottom: f64,
    pub top: f64,
    pub closed: bool,
}
pub fn series(values: &[f64], o: Series) -> Result<String, String> {
    if !(1..=MAX_SAMPLES).contains(&o.samples)
        || values.len() > MAX_SAMPLES
        || [o.width, o.height, o.bottom, o.top]
            .iter()
            .any(|n| !n.is_finite())
        || o.width <= 0.0
        || o.height <= 0.0
        || o.width > 1e6
        || o.height > 1e6
        || values.iter().any(|v| !v.is_finite())
    {
        return Err("invalid graph dimensions or samples".into());
    }
    if values.len() < 2 {
        return Ok("M0 0".into());
    }
    let step = o.width / o.samples.saturating_sub(1).max(1) as f64;
    let offset = o.samples as f64 - values.len() as f64;
    let span = (o.top - o.bottom).max(1e-9);
    let mut path = String::with_capacity(values.len() * 24);
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            path.push(' ');
        }
        let x = (offset + i as f64) * step;
        let y = o.height - 1.0 - ((v - o.bottom) / span).clamp(0.0, 1.0) * (o.height - 2.0);
        let _ = write!(path, "{}{x:.1} {y:.1}", if i == 0 { 'M' } else { 'L' });
    }
    if o.closed {
        let _ = write!(
            path,
            " L{:.1} {:.1} L{:.1} {:.1} Z",
            o.width,
            o.height,
            offset * step,
            o.height
        );
    }
    Ok(path)
}
pub fn grid(w: f64, h: f64, columns: usize, rows: usize) -> Result<String, String> {
    if !w.is_finite()
        || !h.is_finite()
        || w <= 0.0
        || h <= 0.0
        || w > 1e6
        || h > 1e6
        || !(1..=256).contains(&columns)
        || !(1..=256).contains(&rows)
    {
        return Err("invalid graph grid".into());
    }
    let mut parts = Vec::with_capacity(columns + rows - 2);
    for i in 1..columns {
        parts.push(format!("M{:.1} 0 V{h:.1}", w * i as f64 / columns as f64));
    }
    for i in 1..rows {
        parts.push(format!("M0 {:.1} H{w:.1}", h * i as f64 / rows as f64));
    }
    Ok(parts.join(" "))
}
