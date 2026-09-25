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

// ---- The solver: HCT to sRGB, keeping hue and tone. ----

/// The matrices between linear sRGB and "scaled discount" cone responses
/// (adapted by the viewing conditions and scaled by F_L / 100), and the
/// sRGB values that fall exactly between two 8-bit codes.
struct Solver {
    scaled_discount_from_linrgb: [[f64; 3]; 3],
    linrgb_from_scaled_discount: [[f64; 3]; 3],
    critical_planes: [f64; 255],
}

fn solver() -> &'static Solver {
    static SOLVER: OnceLock<Solver> = OnceLock::new();
    SOLVER.get_or_init(|| {
        let vc = ViewingConditions::standard();
        let mut matrix = [[0.0; 3]; 3];
        for (row, out) in matrix.iter_mut().enumerate() {
            for (column, cell) in out.iter_mut().enumerate() {
                let cone_row: f64 = (0..3)
                    .map(|k| CAM16_FROM_XYZ[row][k] * XYZ_FROM_LINRGB[k][column])
                    .sum();
                *cell = cone_row * vc.rgb_d[row] * vc.fl / 100.0;
            }
        }
        let mut critical_planes = [0.0; 255];
        for (index, plane) in critical_planes.iter_mut().enumerate() {
            *plane = linearized((index as f64 + 0.5) / 255.0);
        }
        Solver {
            scaled_discount_from_linrgb: matrix,
            linrgb_from_scaled_discount: invert(&matrix),
            critical_planes,
        }
    })
}

fn sanitize_radians(angle: f64) -> f64 {
    (angle + PI * 8.0) % (PI * 2.0)
}

/// A linear channel (0–100) as an 8-bit code, unrounded.
fn true_delinearized(linear: f64) -> f64 {
    delinearized(linear) * 255.0
}

fn chromatic_adaptation(component: f64) -> f64 {
    let af = component.abs().powf(0.42);
    component.signum() * 400.0 * af / (af + 27.13)
}

fn inverse_chromatic_adaptation(adapted: f64) -> f64 {
    let adapted_abs = adapted.abs();
    let base = (27.13 * adapted_abs / (400.0 - adapted_abs)).max(0.0);
    adapted.signum() * base.powf(1.0 / 0.42)
}

/// The CAM16 hue, in radians, of a linear sRGB colour.
fn hue_of(linrgb: [f64; 3]) -> f64 {
    let scaled = multiply(linrgb, &solver().scaled_discount_from_linrgb);
    let r_a = chromatic_adaptation(scaled[0]);
    let g_a = chromatic_adaptation(scaled[1]);
    let b_a = chromatic_adaptation(scaled[2]);
    let a = (11.0 * r_a - 12.0 * g_a + b_a) / 11.0;
    let b = (r_a + g_a - 2.0 * b_a) / 9.0;
    b.atan2(a)
}

fn are_in_cyclic_order(a: f64, b: f64, c: f64) -> bool {
    sanitize_radians(b - a) < sanitize_radians(c - a)
}

/// The point on the segment from `source` to `target` whose `axis` is
/// `coordinate`.
fn set_coordinate(source: [f64; 3], coordinate: f64, target: [f64; 3], axis: usize) -> [f64; 3] {
    let t = (coordinate - source[axis]) / (target[axis] - source[axis]);
    [0, 1, 2].map(|i| source[i] + (target[i] - source[i]) * t)
}

fn is_bounded(x: f64) -> bool {
    (0.0..=100.0).contains(&x)
}

/// The nth of the (up to twelve) points where the plane of luminance `y`
/// crosses an edge of the RGB cube, or `None`.
fn nth_vertex(y: f64, n: usize) -> Option<[f64; 3]> {
    let [k_r, k_g, k_b] = Y_FROM_LINRGB;
    let coord_a = if n % 4 <= 1 { 0.0 } else { 100.0 };
    let coord_b = if n.is_multiple_of(2) { 0.0 } else { 100.0 };
    let point = if n < 4 {
        let (g, b) = (coord_a, coord_b);
        [(y - g * k_g - b * k_b) / k_r, g, b]
    } else if n < 8 {
        let (b, r) = (coord_a, coord_b);
        [r, (y - r * k_r - b * k_b) / k_g, b]
    } else {
        let (r, g) = (coord_a, coord_b);
        [r, g, (y - r * k_r - g * k_g) / k_b]
    };
    let axis = if n < 4 {
        0
    } else if n < 8 {
        1
    } else {
        2
    };
    is_bounded(point[axis]).then_some(point)
}

/// The two vertices of the constant-luminance polygon whose hues bracket
/// `target_hue`.
fn bisect_to_segment(y: f64, target_hue: f64) -> ([f64; 3], [f64; 3]) {
    let mut left = [-1.0; 3];
    let mut right = left;
    let mut left_hue = 0.0;
    let mut right_hue = 0.0;
    let mut initialized = false;
    let mut uncut = true;
    for n in 0..12 {
        let Some(mid) = nth_vertex(y, n) else {
            continue;
        };
        let mid_hue = hue_of(mid);
        if !initialized {
            left = mid;
            right = mid;
            left_hue = mid_hue;
            right_hue = mid_hue;
            initialized = true;
            continue;
        }
        if uncut || are_in_cyclic_order(left_hue, mid_hue, right_hue) {
            uncut = false;
            if are_in_cyclic_order(left_hue, target_hue, mid_hue) {
                right = mid;
                right_hue = mid_hue;
            } else {
                left = mid;
                left_hue = mid_hue;
            }
        }
    }
    (left, right)
}

fn critical_plane_below(x: f64) -> i32 {
    (x - 0.5).floor() as i32
}

fn critical_plane_above(x: f64) -> i32 {
    (x - 0.5).ceil() as i32
}

/// The colour of luminance `y` on the gamut's surface with hue closest to
/// `target_hue`: the most chroma sRGB has at that hue and tone.
fn bisect_to_limit(y: f64, target_hue: f64) -> [f64; 3] {
    let planes = &solver().critical_planes;
    let (mut left, mut right) = bisect_to_segment(y, target_hue);
    let mut left_hue = hue_of(left);
    for axis in 0..3 {
        if left[axis] == right[axis] {
            continue;
        }
        let (mut l_plane, mut r_plane) = if left[axis] < right[axis] {
            (
                critical_plane_below(true_delinearized(left[axis])),
                critical_plane_above(true_delinearized(right[axis])),
            )
        } else {
            (
                critical_plane_above(true_delinearized(left[axis])),
                critical_plane_below(true_delinearized(right[axis])),
            )
        };
        for _ in 0..8 {
            if (r_plane - l_plane).abs() <= 1 {
                break;
            }
            let m_plane = ((l_plane + r_plane) as f64 / 2.0).floor() as i32;
            let coordinate = planes[m_plane.clamp(0, 254) as usize];
            let mid = set_coordinate(left, coordinate, right, axis);
            let mid_hue = hue_of(mid);
            if are_in_cyclic_order(left_hue, target_hue, mid_hue) {
                right = mid;
                r_plane = m_plane;
            } else {
                left = mid;
                left_hue = mid_hue;
                l_plane = m_plane;
            }
        }
    }
    [0, 1, 2].map(|i| (left[i] + right[i]) / 2.0)
}

/// Newton's method on J for the exact colour of this hue, chroma and
/// luminance; `None` when it falls outside sRGB.
fn find_result_by_j(hue_radians: f64, chroma: f64, y: f64) -> Option<[f64; 3]> {
    let vc = ViewingConditions::standard();
    let mut j = y.sqrt() * 11.0;
    let t_inner_coeff = 1.0 / (1.64 - 0.29f64.powf(vc.n)).powf(0.73);
    let e_hue = 0.25 * ((hue_radians + 2.0).cos() + 3.8);
    let p1 = e_hue * (50000.0 / 13.0) * vc.nc * vc.ncb;
    let (h_sin, h_cos) = hue_radians.sin_cos();
    for round in 0..5 {
        let j_normalized = j / 100.0;
        let alpha = if chroma == 0.0 || j == 0.0 {
            0.0
        } else {
            chroma / j_normalized.sqrt()
        };
        let t = (alpha * t_inner_coeff).powf(1.0 / 0.9);
        let ac = vc.aw * j_normalized.powf(1.0 / vc.c / vc.z);
        let p2 = ac / vc.nbb;
        let gamma = 23.0 * (p2 + 0.305) * t / (23.0 * p1 + 11.0 * t * h_cos + 108.0 * t * h_sin);
        let a = gamma * h_cos;
        let b = gamma * h_sin;
        let r_a = (460.0 * p2 + 451.0 * a + 288.0 * b) / 1403.0;
        let g_a = (460.0 * p2 - 891.0 * a - 261.0 * b) / 1403.0;
        let b_a = (460.0 * p2 - 220.0 * a - 6300.0 * b) / 1403.0;
        let scaled = [
            inverse_chromatic_adaptation(r_a),
            inverse_chromatic_adaptation(g_a),
            inverse_chromatic_adaptation(b_a),
        ];
        let linrgb = multiply(scaled, &solver().linrgb_from_scaled_discount);
        if linrgb.iter().any(|&channel| channel < 0.0) {
            return None;
        }
        let fnj = linrgb[0] * Y_FROM_LINRGB[0]
            + linrgb[1] * Y_FROM_LINRGB[1]
            + linrgb[2] * Y_FROM_LINRGB[2];
        if fnj <= 0.0 {
            return None;
        }
        if round == 4 || (fnj - y).abs() < 0.002 {
            if linrgb.iter().any(|&channel| channel > 100.01) {
                return None;
            }
            return Some(linrgb);
        }
        // Newton's method on the square root of J.
        j -= (fnj - y) * j / (2.0 * fnj);
    }
    None
}

/// The sRGB colour (channels 0–1) of this hue (degrees), chroma and tone
/// (0–100). Tone and hue are kept; a chroma sRGB cannot show at that hue
/// and tone comes back as the most it can.
pub fn solve(hue: f64, chroma: f64, tone: f64) -> [f64; 3] {
    if !(hue.is_finite() && chroma.is_finite() && tone.is_finite()) {
        return [0.0; 3];
    }
    if chroma < 0.0001 || !(0.0001..=99.9999).contains(&tone) {
        let gray = delinearized(y_from_lstar(tone.clamp(0.0, 100.0))).clamp(0.0, 1.0);
        return [gray; 3];
    }
    let hue_radians = sanitize_degrees(hue).to_radians();
    let y = y_from_lstar(tone);
    let linrgb =
        find_result_by_j(hue_radians, chroma, y).unwrap_or_else(|| bisect_to_limit(y, hue_radians));
    linrgb.map(|channel| delinearized(channel.clamp(0.0, 100.0)).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn srgb(hex: u32) -> [f64; 3] {
        [16, 8, 0].map(|shift| f64::from((hex >> shift) & 0xff) / 255.0)
    }

    fn hex(rgb: [f64; 3]) -> u32 {
        rgb.iter().fold(0, |acc, channel| {
            (acc << 8) | (channel * 255.0).round() as u32
        })
    }

    fn cam(hex: u32) -> Cam16 {
        Cam16::from_linrgb(srgb(hex).map(linearized), ViewingConditions::standard())
    }

    fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{what}: {actual} is not {expected} ± {tolerance}"
        );
    }

    #[test]
    fn cam16_of_the_primaries_matches_the_reference() {
        // Published in Material Color Utilities' cam16 tests.
        let red = cam(0xff0000);
        close(red.hue, 27.408, 0.001, "red hue");
        close(red.chroma, 113.357, 0.001, "red chroma");
        close(red.j, 46.445, 0.001, "red J");
        close(red.m, 89.494, 0.001, "red M");
        close(red.s, 91.889, 0.001, "red s");
        close(red.q, 105.988, 0.001, "red Q");
        let green = cam(0x00ff00);
        close(green.hue, 142.139, 0.001, "green hue");
        close(green.chroma, 108.410, 0.001, "green chroma");
        close(green.j, 79.331, 0.001, "green J");
        close(green.m, 85.587, 0.001, "green M");
        close(green.s, 78.604, 0.001, "green s");
        close(green.q, 138.520, 0.001, "green Q");
        let blue = cam(0x0000ff);
        close(blue.hue, 282.788, 0.001, "blue hue");
        close(blue.chroma, 87.230, 0.001, "blue chroma");
        close(blue.j, 25.465, 0.001, "blue J");
        close(blue.m, 68.867, 0.001, "blue M");
        close(blue.s, 93.674, 0.001, "blue s");
        close(blue.q, 78.481, 0.001, "blue Q");
        let white = cam(0xffffff);
        close(white.hue, 209.492, 0.001, "white hue");
        close(white.chroma, 2.869, 0.001, "white chroma");
        close(white.j, 100.0, 0.001, "white J");
        let black = cam(0x000000);
        close(black.chroma, 0.0, 0.001, "black chroma");
        close(black.j, 0.0, 0.001, "black J");
    }

    #[test]
    fn tone_is_lstar() {
        close(hct_from_srgb(srgb(0xff0000))[2], 53.233, 0.01, "red tone");
        close(hct_from_srgb(srgb(0x00ff00))[2], 87.737, 0.01, "green tone");
        close(hct_from_srgb(srgb(0x0000ff))[2], 32.303, 0.01, "blue tone");
        close(hct_from_srgb(srgb(0x808080))[2], 53.585, 0.01, "grey tone");
    }

    #[test]
    fn derived_solver_matrices_match_the_published_ones() {
        // The constants Material Color Utilities' HctSolver ships with.
        let published = [
            [
                0.001200833568784504,
                0.002389694492170889,
                0.0002795742885861124,
            ],
            [
                0.0005891086651375999,
                0.0029785502573438758,
                0.0003270666104008398,
            ],
            [
                0.00010146692491640572,
                0.0005364214359186694,
                0.0032979401770712076,
            ],
        ];
        let inverse = [
            [1373.2198709594231, -1100.4251190754821, -7.278681089101213],
            [-271.815969077903, 559.6580465940733, -32.46047482791194],
            [1.9622899599665666, -57.173814538844006, 308.7233197812385],
        ];
        let solver = solver();
        for row in 0..3 {
            for column in 0..3 {
                let ours = solver.scaled_discount_from_linrgb[row][column];
                let theirs = published[row][column];
                assert!(
                    ((ours - theirs) / theirs).abs() < 1e-6,
                    "[{row}][{column}] {ours} vs {theirs}"
                );
                let ours = solver.linrgb_from_scaled_discount[row][column];
                let theirs = inverse[row][column];
                assert!(
                    ((ours - theirs) / theirs).abs() < 1e-6,
                    "inverse [{row}][{column}] {ours} vs {theirs}"
                );
            }
        }
        close(
            solver.critical_planes[0],
            0.015176349177441876,
            1e-12,
            "first plane",
        );
    }

    #[test]
    fn every_srgb_colour_round_trips() {
        // Through HCT and back to the same 8-bit colour, across the cube.
        for r in (0..=255).step_by(15) {
            for g in (0..=255).step_by(15) {
                for b in (0..=255).step_by(15) {
                    let colour = (r << 16) | (g << 8) | b;
                    let [h, c, t] = hct_from_srgb(srgb(colour));
                    let back = hex(solve(h, c, t));
                    assert_eq!(
                        back, colour,
                        "{colour:06x} via hct {h} {c} {t} came back {back:06x}"
                    );
                }
            }
        }
    }

    #[test]
    fn asking_for_too_much_chroma_keeps_hue_and_tone() {
        // Material Color Utilities' own check: over hues, chromas and
        // tones, the colour made has the tone asked for, the hue asked for
        // (when it has any chroma to show it), and no more chroma.
        for hue in (15..360).step_by(30) {
            for chroma in (0..=100).step_by(10) {
                for tone in (20..=80).step_by(10) {
                    let (hue, chroma, tone) = (f64::from(hue), f64::from(chroma), f64::from(tone));
                    let [h, c, t] = hct_from_srgb(solve(hue, chroma, tone));
                    if chroma > 0.0 {
                        let difference = (h - hue).abs().min(360.0 - (h - hue).abs());
                        assert!(difference <= 4.0, "hue {hue} {chroma} {tone} came back {h}");
                    }
                    assert!(
                        c <= chroma + 2.5,
                        "chroma {hue} {chroma} {tone} came back {c}"
                    );
                    close(t, tone, 0.5, "tone");
                }
            }
        }
    }

    #[test]
    fn tonal_palette_of_blue_matches_the_reference() {
        // Material Color Utilities' palettes test: the tones of #0000ff.
        let [hue, chroma, _] = hct_from_srgb(srgb(0x0000ff));
        let expected = [
            (100, 0xffffff),
            (95, 0xf1efff),
            (90, 0xe0e0ff),
            (80, 0xbec2ff),
            (70, 0x9da3ff),
            (60, 0x7c84ff),
            (50, 0x5a64ff),
            (40, 0x343dff),
            (30, 0x0000ef),
            (20, 0x0001ac),
            (10, 0x00006e),
            (0, 0x000000),
        ];
        for (tone, colour) in expected {
            let made = hex(solve(hue, chroma, f64::from(tone)));
            assert_eq!(made, colour, "tone {tone}: {made:06x} not {colour:06x}");
        }
    }
}
