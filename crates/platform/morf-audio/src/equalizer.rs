//! Eight-band stereo EQ prescription and response, independent of UI/server.
//! Audiogram mapping follows Fail-Safe/omarchy-audiogram-eq (MIT): half of
//! threshold loss above 20 dB HL, capped at 12 dB, scaled by strength.
use std::f64::consts::PI;
pub const FREQUENCIES: [f64; 8] = [250., 500., 1000., 2000., 3000., 4000., 6000., 8000.];
pub const POINTS: usize = 160;
#[derive(Clone, Debug)]
pub struct Options {
    pub left: [f64; 8],
    pub right: [f64; 8],
    pub bands: [f64; 8],
    pub strength: f64,
    pub per_ear: bool,
    pub compensation: bool,
    pub enabled: bool,
    pub trim: f64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            left: [0.; 8],
            right: [0.; 8],
            bands: [0.; 8],
            strength: 30.,
            per_ear: true,
            compensation: false,
            enabled: true,
            trim: 0.,
        }
    }
}
pub struct Curve {
    pub left: [f64; 8],
    pub right: [f64; 8],
    pub preamp: f64,
    pub response_left: Vec<f64>,
    pub response_right: Vec<f64>,
}
fn valid(values: &[f64], low: f64, high: f64) -> bool {
    values
        .iter()
        .all(|v| v.is_finite() && (low..=high).contains(v))
}
// RBJ biquads at the graph's fixed 48 kHz rate. Ends are shelves, middle
// bands peaking filters, Q=1. The displayed response includes their overlap.
fn coefficients(index: usize, gain: f64) -> [f64; 6] {
    let a = 10_f64.powf(gain / 40.);
    let w = 2. * PI * FREQUENCIES[index] / 48000.;
    let c = w.cos();
    let alpha = w.sin() / 2.;
    let beta = 2. * a.sqrt() * alpha;
    match index {
        0 => [
            a * ((a + 1.) - (a - 1.) * c + beta),
            2. * a * ((a - 1.) - (a + 1.) * c),
            a * ((a + 1.) - (a - 1.) * c - beta),
            (a + 1.) + (a - 1.) * c + beta,
            -2. * ((a - 1.) + (a + 1.) * c),
            (a + 1.) + (a - 1.) * c - beta,
        ],
        7 => [
            a * ((a + 1.) + (a - 1.) * c + beta),
            -2. * a * ((a - 1.) + (a + 1.) * c),
            a * ((a + 1.) + (a - 1.) * c - beta),
            (a + 1.) - (a - 1.) * c + beta,
            2. * ((a - 1.) - (a + 1.) * c),
            (a + 1.) - (a - 1.) * c - beta,
        ],
        _ => [
            1. + alpha * a,
            -2. * c,
            1. - alpha * a,
            1. + alpha / a,
            -2. * c,
            1. - alpha / a,
        ],
    }
}
fn response(filters: &[[f64; 6]; 8], hz: f64) -> f64 {
    let w = 2. * PI * hz / 48000.;
    let power = |p: &[f64]| {
        let re = p[0] + p[1] * w.cos() + p[2] * (2. * w).cos();
        let im = p[1] * w.sin() + p[2] * (2. * w).sin();
        re * re + im * im
    };
    filters
        .iter()
        .map(|p| 10. * (power(&p[..3]) / power(&p[3..])).log10())
        .sum()
}
pub fn curve(o: &Options) -> Result<Curve, String> {
    if !valid(&o.left, -10., 120.)
        || !valid(&o.right, -10., 120.)
        || !valid(&o.bands, -12., 12.)
        || !valid(&[o.strength], 0., 100.)
        || !valid(&[o.trim], -24., 0.)
    {
        return Err("EQ expects eight finite thresholds (-10..120), bands (-12..12), strength (0..100), trim (-24..0)".into());
    }
    let gain = |t: f64| (0.5 * (t - 20.)).clamp(0., 12.) * o.strength / 100.;
    let mut left = [0.; 8];
    let mut right = [0.; 8];
    for i in 0..8 {
        if o.enabled {
            let l = if o.per_ear {
                o.left[i]
            } else {
                (o.left[i] + o.right[i]) / 2.
            };
            let r = if o.per_ear { o.right[i] } else { l };
            left[i] = o.bands[i] + if o.compensation { gain(l) } else { 0. };
            right[i] = o.bands[i] + if o.compensation { gain(r) } else { 0. };
        }
    }
    let lf = std::array::from_fn(|i| coefficients(i, left[i]));
    let rf = std::array::from_fn(|i| coefficients(i, right[i]));
    // Dense log sweep covers overlapping boosts, with half a dB margin.
    // This is headroom estimation, not a brick-wall limiter.
    let mut peak: f64 = 0.;
    for i in 0..1024 {
        let hz = 10. * 2200_f64.powf(i as f64 / 1023.);
        peak = peak.max(response(&lf, hz)).max(response(&rf, hz));
    }
    let preamp = if o.enabled {
        o.trim - if peak > 0.001 { peak + 0.5 } else { 0. }
    } else {
        0.
    };
    let line = |f: &[[f64; 6]; 8]| {
        (0..POINTS)
            .map(|i| response(f, 20. * 1000_f64.powf(i as f64 / (POINTS - 1) as f64)) + preamp)
            .collect()
    };
    Ok(Curve {
        left,
        right,
        preamp,
        response_left: line(&lf),
        response_right: line(&rf),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_and_averaged_ears() {
        let mut o = Options {
            left: [60.; 8],
            strength: 50.,
            compensation: true,
            ..Options::default()
        };
        let c = curve(&o).unwrap();
        assert_eq!(c.left, [6.; 8]);
        assert_eq!(c.right, [0.; 8]);
        o.per_ear = false;
        let c = curve(&o).unwrap();
        assert_eq!(c.left, [2.5; 8]);
        assert_eq!(c.right, c.left);
    }
    #[test]
    fn overlap_headroom_and_bypass() {
        let mut o = Options {
            bands: [9.; 8],
            ..Options::default()
        };
        let c = curve(&o).unwrap();
        assert!(c.preamp < -9.);
        assert!(c.response_left.iter().all(|v| *v <= 0.));
        o.enabled = false;
        let c = curve(&o).unwrap();
        assert_eq!(c.preamp, 0.);
        assert!(c.response_left.iter().all(|v| v.abs() < 1e-8));
    }
    #[test]
    fn reject_invalid_and_flat_is_neutral() {
        let mut o = Options::default();
        assert_eq!(curve(&o).unwrap().preamp, 0.);
        o.left[3] = f64::NAN;
        assert!(curve(&o).is_err());
        o.left[3] = 121.;
        assert!(curve(&o).is_err());
    }
}
