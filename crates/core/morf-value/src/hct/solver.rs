//! The solver: HCT to sRGB, keeping hue and tone, and giving up chroma
//! where sRGB cannot show it.

use std::f64::consts::PI;
use std::sync::OnceLock;

use super::{
    CAM16_FROM_XYZ, ViewingConditions, XYZ_FROM_LINRGB, Y_FROM_LINRGB, delinearized, invert,
    linearized, multiply, sanitize_degrees, y_from_lstar,
};

/// The matrices between linear sRGB and "scaled discount" cone responses
/// (adapted by the viewing conditions and scaled by F_L / 100), and the
/// sRGB values that fall exactly between two 8-bit codes.
pub(super) struct Solver {
    pub(super) scaled_discount_from_linrgb: [[f64; 3]; 3],
    pub(super) linrgb_from_scaled_discount: [[f64; 3]; 3],
    pub(super) critical_planes: [f64; 255],
}

pub(super) fn solver() -> &'static Solver {
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
