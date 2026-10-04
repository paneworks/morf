//! HCT: hue and chroma from CAM16, tone from CIE L*.
//!
//! The colour space Material Design builds its schemes in. Hue and chroma
//! are CAM16's under the default viewing conditions (a grey world, average
//! surround), so equal steps look equal; tone is L*, so a tone difference
//! is a contrast ratio regardless of hue. Asking for a colour sRGB cannot
//! show keeps its hue and tone and gives up chroma: the gamut mapping
//! [`solve`] does.
//!
//! Ported from Google's Material Color Utilities (the `hct/cam16`,
//! `hct/viewing_conditions` and `hct/hct_solver` sources, and
//! `palettes/tonal_palette`), Copyright 2021–2022 Google LLC, licensed
//! under the Apache License, Version 2.0
//! (<https://www.apache.org/licenses/LICENSE-2.0>). The algorithms and
//! numeric constants are theirs; this is a Rust rendering of them, and the
//! matrices the solver uses are derived from the viewing conditions here
//! rather than copied (a test checks them against the published values).

mod solver;
#[cfg(test)]
mod tests;

pub use solver::solve;

use std::f64::consts::PI;
use std::sync::OnceLock;

/// Linear sRGB (0–100) to XYZ (D65, 0–100).
const XYZ_FROM_LINRGB: [[f64; 3]; 3] = [
    [0.41233895, 0.35762064, 0.18051042],
    [0.2126, 0.7152, 0.0722],
    [0.01932141, 0.11916382, 0.95034478],
];

/// XYZ to CAM16's cone space.
const CAM16_FROM_XYZ: [[f64; 3]; 3] = [
    [0.401288, 0.650173, -0.051461],
    [-0.250268, 1.204414, 0.045854],
    [-0.002079, 0.048952, 0.953127],
];

const Y_FROM_LINRGB: [f64; 3] = [0.2126, 0.7152, 0.0722];

const WHITE_POINT_D65: [f64; 3] = [95.047, 100.0, 108.883];

fn multiply(row: [f64; 3], matrix: &[[f64; 3]; 3]) -> [f64; 3] {
    [
        row[0] * matrix[0][0] + row[1] * matrix[0][1] + row[2] * matrix[0][2],
        row[0] * matrix[1][0] + row[1] * matrix[1][1] + row[2] * matrix[1][2],
        row[0] * matrix[2][0] + row[1] * matrix[2][1] + row[2] * matrix[2][2],
    ]
}

fn invert(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let cofactor =
        |r0: usize, c0: usize, r1: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
    [
        [
            cofactor(1, 1, 2, 2) / det,
            -cofactor(0, 1, 2, 2) / det,
            cofactor(0, 1, 1, 2) / det,
        ],
        [
            -cofactor(1, 0, 2, 2) / det,
            cofactor(0, 0, 2, 2) / det,
            -cofactor(0, 0, 1, 2) / det,
        ],
        [
            cofactor(1, 0, 2, 1) / det,
            -cofactor(0, 0, 2, 1) / det,
            cofactor(0, 0, 1, 1) / det,
        ],
    ]
}

/// An sRGB channel, 0–1, to linear light, 0–100.
pub fn linearized(channel: f64) -> f64 {
    let linear = if channel <= 0.040449936 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    };
    linear * 100.0
}

/// Linear light, 0–100, to an sRGB channel, 0–1 (unclamped).
pub fn delinearized(linear: f64) -> f64 {
    let normalized = linear / 100.0;
    if normalized <= 0.0031308 {
        normalized * 12.92
    } else {
        1.055 * normalized.powf(1.0 / 2.4) - 0.055
    }
}

fn lab_f(t: f64) -> f64 {
    let e = 216.0 / 24389.0;
    let kappa = 24389.0 / 27.0;
    if t > e {
        t.cbrt()
    } else {
        (kappa * t + 16.0) / 116.0
    }
}

fn lab_inverse_f(ft: f64) -> f64 {
    let e = 216.0 / 24389.0;
    let kappa = 24389.0 / 27.0;
    let cubed = ft * ft * ft;
    if cubed > e {
        cubed
    } else {
        (116.0 * ft - 16.0) / kappa
    }
}

/// L* (0–100) to relative luminance Y (0–100).
pub fn y_from_lstar(lstar: f64) -> f64 {
    100.0 * lab_inverse_f((lstar + 16.0) / 116.0)
}

/// Relative luminance Y (0–100) to L* (0–100).
pub fn lstar_from_y(y: f64) -> f64 {
    lab_f(y / 100.0) * 116.0 - 16.0
}

fn sanitize_degrees(degrees: f64) -> f64 {
    let degrees = degrees % 360.0;
    if degrees < 0.0 {
        degrees + 360.0
    } else {
        degrees
    }
}

fn lerp(start: f64, stop: f64, amount: f64) -> f64 {
    (1.0 - amount) * start + amount * stop
}

/// The environment a colour is seen in.
#[derive(Clone, Debug)]
pub struct ViewingConditions {
    pub n: f64,
    pub aw: f64,
    pub nbb: f64,
    pub ncb: f64,
    pub c: f64,
    pub nc: f64,
    pub rgb_d: [f64; 3],
    pub fl: f64,
    pub fl_root: f64,
    pub z: f64,
}

impl ViewingConditions {
    /// Material's default: D65 white, a grey world (background L* 50), an
    /// adapting luminance of 200/π·Y(50)/100, average surround, the
    /// illuminant not discounted.
    pub fn standard() -> &'static Self {
        static STANDARD: OnceLock<ViewingConditions> = OnceLock::new();
        STANDARD.get_or_init(|| {
            Self::make(
                WHITE_POINT_D65,
                (200.0 / PI) * y_from_lstar(50.0) / 100.0,
                50.0,
                2.0,
                false,
            )
        })
    }

    pub fn make(
        white: [f64; 3],
        adapting_luminance: f64,
        background_lstar: f64,
        surround: f64,
        discounting_illuminant: bool,
    ) -> Self {
        let background_lstar = background_lstar.max(0.1);
        let [r_w, g_w, b_w] = multiply(white, &CAM16_FROM_XYZ);
        let f = 0.8 + surround / 10.0;
        let c = if f >= 0.9 {
            lerp(0.59, 0.69, (f - 0.9) * 10.0)
        } else {
            lerp(0.525, 0.59, (f - 0.8) * 10.0)
        };
        let d = if discounting_illuminant {
            1.0
        } else {
            f * (1.0 - (1.0 / 3.6) * ((-adapting_luminance - 42.0) / 92.0).exp())
        }
        .clamp(0.0, 1.0);
        let nc = f;
        let rgb_d = [
            d * (100.0 / r_w) + 1.0 - d,
            d * (100.0 / g_w) + 1.0 - d,
            d * (100.0 / b_w) + 1.0 - d,
        ];
        let k = 1.0 / (5.0 * adapting_luminance + 1.0);
        let k4 = k * k * k * k;
        let k4f = 1.0 - k4;
        let fl = k4 * adapting_luminance + 0.1 * k4f * k4f * (5.0 * adapting_luminance).cbrt();
        let n = y_from_lstar(background_lstar) / white[1];
        let z = 1.48 + n.sqrt();
        let nbb = 0.725 / n.powf(0.2);
        let ncb = nbb;
        let adapted = [r_w, g_w, b_w]
            .iter()
            .zip(rgb_d)
            .map(|(white, d)| {
                let factor = (fl * d * white / 100.0).powf(0.42);
                400.0 * factor / (factor + 27.13)
            })
            .collect::<Vec<_>>();
        let aw = (2.0 * adapted[0] + adapted[1] + 0.05 * adapted[2]) * nbb;
        Self {
            n,
            aw,
            nbb,
            ncb,
            c,
            nc,
            rgb_d,
            fl,
            fl_root: fl.powf(0.25),
            z,
        }
    }
}

/// A colour's CAM16 appearance: hue (degrees), chroma, lightness J,
/// brightness Q, colourfulness M, saturation s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cam16 {
    pub hue: f64,
    pub chroma: f64,
    pub j: f64,
    pub q: f64,
    pub m: f64,
    pub s: f64,
}

impl Cam16 {
    /// From linear sRGB, 0–100 per channel.
    pub fn from_linrgb(linrgb: [f64; 3], vc: &ViewingConditions) -> Self {
        let xyz = multiply(linrgb, &XYZ_FROM_LINRGB);
        let cone = multiply(xyz, &CAM16_FROM_XYZ);
        let adapted = [0, 1, 2].map(|i| {
            let d = vc.rgb_d[i] * cone[i];
            let af = (vc.fl * d.abs() / 100.0).powf(0.42);
            d.signum() * 400.0 * af / (af + 27.13)
        });
        let [r_a, g_a, b_a] = adapted;
        let a = (11.0 * r_a - 12.0 * g_a + b_a) / 11.0;
        let b = (r_a + g_a - 2.0 * b_a) / 9.0;
        let u = (20.0 * r_a + 20.0 * g_a + 21.0 * b_a) / 20.0;
        let p2 = (40.0 * r_a + 20.0 * g_a + b_a) / 20.0;
        let hue = sanitize_degrees(b.atan2(a).to_degrees());
        let ac = p2 * vc.nbb;
        let j = 100.0 * (ac / vc.aw).powf(vc.c * vc.z);
        let q = 4.0 / vc.c * (j / 100.0).sqrt() * (vc.aw + 4.0) * vc.fl_root;
        let hue_prime = if hue < 20.14 { hue + 360.0 } else { hue };
        let e_hue = 0.25 * ((hue_prime.to_radians() + 2.0).cos() + 3.8);
        let p1 = 50000.0 / 13.0 * e_hue * vc.nc * vc.ncb;
        let t = p1 * a.hypot(b) / (u + 0.305);
        let alpha = t.powf(0.9) * (1.64 - 0.29f64.powf(vc.n)).powf(0.73);
        let chroma = alpha * (j / 100.0).sqrt();
        let m = chroma * vc.fl_root;
        let s = 50.0 * ((alpha * vc.c) / (vc.aw + 4.0)).sqrt();
        Self {
            hue,
            chroma,
            j,
            q,
            m,
            s,
        }
    }
}

/// An sRGB colour (channels 0–1) as HCT: hue in degrees, chroma, tone.
pub fn hct_from_srgb(rgb: [f64; 3]) -> [f64; 3] {
    let linrgb = rgb.map(|channel| linearized(channel.clamp(0.0, 1.0)));
    let cam = Cam16::from_linrgb(linrgb, ViewingConditions::standard());
    let y =
        linrgb[0] * Y_FROM_LINRGB[0] + linrgb[1] * Y_FROM_LINRGB[1] + linrgb[2] * Y_FROM_LINRGB[2];
    [cam.hue, cam.chroma, lstar_from_y(y)]
}
