//! HDR output (`MORF_HDR=1`): frames described to the compositor as PQ or
//! scRGB rather than sRGB, so a colour can be brighter than SDR white.
//!
//! Content is authored as before -- sRGB colours, blended in linear light
//! or in sRGB -- and only the composite at the end of the frame changes:
//!
//! - PQ (ST 2084) over BT.2020, for a ten-bit buffer: linear sRGB light is
//!   taken to BT.2020 primaries, SDR white placed at the reference white
//!   (203 cd/m² unless `MORF_HDR_WHITE` says otherwise), and PQ-encoded;
//! - scRGB, for a half-float buffer, where the compositor takes extended
//!   linear: linear sRGB light as it is, 1.0 being SDR white.
//!
//! scRGB is preferred wherever the compositor takes it, opaque surfaces
//! included: a compositor anchors its 1.0 at SDR white, where Hyprland, for
//! one, tone-maps PQ on an SDR output and SDR content comes out changed.
//! `MORF_HDR=pq` keeps opaque surfaces on ten-bit PQ. What the compositor
//! cannot describe falls back: scRGB to PQ, PQ to plain sRGB, the frame as
//! without the setting. A buffer of eight bits is always sRGB.

use std::sync::atomic::{AtomicU8, Ordering};

use morf_value::present::{ColorSupport, Encoding};

use super::depth::PresentFormat;

/// SDR white in PQ's units of luminance, cd/m², unless told otherwise:
/// Report ITU-R BT.2408, and what the protocol assumes for PQ.
pub const DEFAULT_REFERENCE_WHITE: u32 = 203;

/// Whether HDR is asked for, and where SDR white goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HdrRequest {
    pub enabled: bool,
    /// SDR white under PQ, in cd/m².
    pub reference_white: u32,
    /// `MORF_HDR=pq`: an opaque surface presents ten-bit PQ even where the
    /// compositor takes scRGB.
    pub prefer_pq: bool,
}

impl Default for HdrRequest {
    fn default() -> Self {
        Self {
            enabled: false,
            reference_white: DEFAULT_REFERENCE_WHITE,
            prefer_pq: false,
        }
    }
}

impl HdrRequest {
    /// `MORF_HDR=1` (or `pq`) asks; `MORF_HDR_WHITE=N` puts SDR white at N cd/m²
    /// (80 to 1000).
    pub fn from_env() -> Self {
        let read = |name| std::env::var(name).ok();
        Self::parse(
            read("MORF_HDR").as_deref(),
            read("MORF_HDR_WHITE").as_deref(),
        )
    }

    pub fn parse(hdr: Option<&str>, white: Option<&str>) -> Self {
        let hdr = hdr.map(str::trim);
        Self {
            enabled: matches!(hdr, Some("1" | "true" | "on" | "pq")),
            prefer_pq: hdr == Some("pq"),
            reference_white: white
                .and_then(|white| white.trim().parse::<u32>().ok())
                .map_or(DEFAULT_REFERENCE_WHITE, |white| white.clamp(80, 1000)),
        }
    }
}

/// How a frame presented as `format` is encoded, given what is asked and
/// what the compositor can describe.
pub(crate) fn choose_encoding(
    format: PresentFormat,
    support: ColorSupport,
    request: HdrRequest,
) -> Encoding {
    if !request.enabled || !format.deep() {
        return Encoding::Srgb;
    }
    // Without its own luminances, PQ's reference white is the protocol's.
    let reference_white = if support.luminances {
        request.reference_white
    } else {
        DEFAULT_REFERENCE_WHITE
    };
    let pq = Encoding::Pq { reference_white };
    match format {
        PresentFormat::Half if support.can_scrgb() => Encoding::ExtLinear,
        _ if support.can_pq() => pq,
        _ => Encoding::Srgb,
    }
}

/// Linear BT.709 (sRGB) light to linear BT.2020 light, row major.
pub const SRGB_TO_BT2020: [[f64; 3]; 3] = [
    [
        0.627_403_895_934_699,
        0.329_283_038_377_884,
        0.043_313_065_687_417,
    ],
    [
        0.069_097_289_358_232,
        0.919_540_395_075_459,
        0.011_362_315_566_309,
    ],
    [
        0.016_391_438_875_150,
        0.088_013_307_877_226,
        0.895_595_253_247_624,
    ],
];

const PQ_M1: f64 = 2610.0 / 16384.0;
const PQ_M2: f64 = 2523.0 / 4096.0 * 128.0;
const PQ_C1: f64 = 3424.0 / 4096.0;
const PQ_C2: f64 = 2413.0 / 4096.0 * 32.0;
const PQ_C3: f64 = 2392.0 / 4096.0 * 32.0;

/// The PQ signal for `luminance` in cd/m² (0 to 10000).
pub fn pq_encode(luminance: f64) -> f64 {
    let y = (luminance / 10_000.0).clamp(0.0, 1.0);
    let p = y.powf(PQ_M1);
    ((PQ_C1 + PQ_C2 * p) / (1.0 + PQ_C3 * p)).powf(PQ_M2)
}

/// The luminance in cd/m² a PQ signal stands for.
pub fn pq_decode(signal: f64) -> f64 {
    let e = signal.clamp(0.0, 1.0).powf(1.0 / PQ_M2);
    let y = ((e - PQ_C1).max(0.0) / (PQ_C2 - PQ_C3 * e)).powf(1.0 / PQ_M1);
    y * 10_000.0
}

/// The PQ BT.2020 signal for linear sRGB light, SDR white being 1.0 and
/// shown at `reference_white` cd/m²: what the composite writes.
pub fn encode_pq(linear: [f64; 3], reference_white: f64) -> [f64; 3] {
    SRGB_TO_BT2020.map(|row| {
        let wide = row[0] * linear[0] + row[1] * linear[1] + row[2] * linear[2];
        pq_encode(wide.max(0.0) * reference_white)
    })
}

/// Encodings already said, so each is logged once.
static SAID: AtomicU8 = AtomicU8::new(0);

/// Says, once per encoding, what a surface is described as.
pub(crate) fn say(encoding: Encoding, advertised: &str, why: Option<&str>) {
    let bit = match encoding {
        Encoding::Srgb => 1,
        Encoding::Pq { .. } => 2,
        Encoding::ExtLinear => 4,
    };
    if SAID.fetch_or(bit, Ordering::Relaxed) & bit != 0 {
        return;
    }
    let described = match encoding {
        Encoding::Srgb => "sRGB (no HDR)".to_owned(),
        Encoding::Pq { reference_white } => {
            format!("PQ over BT.2020, SDR white at {reference_white} cd/m²")
        }
        Encoding::ExtLinear => "extended linear sRGB (scRGB, 1.0 = SDR white)".to_owned(),
    };
    let why = why.map(|why| format!(" ({why})")).unwrap_or_default();
    eprintln!("morf: gpu: hdr asked; {advertised}; surface described as {described}{why}");
}

/// What a compositor's colour manager offers, for the log.
pub(crate) fn advertised(support: ColorSupport) -> String {
    if support == ColorSupport::default() {
        "no wp_color_manager_v1 colour support".to_owned()
    } else {
        format!(
            "wp_color_manager_v1: parametric {}, luminances {}, pq {}, ext_linear {}, bt2020 {}, srgb {}",
            support.parametric,
            support.luminances,
            support.pq,
            support.ext_linear,
            support.bt2020,
            support.srgb_primaries
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, by: f64) -> bool {
        (a - b).abs() <= by
    }

    #[test]
    fn pq_meets_its_published_points() {
        assert_eq!(pq_encode(0.0), PQ_C1.powf(PQ_M2));
        assert!(close(pq_encode(10_000.0), 1.0, 1e-12));
        // BT.2408: 203 cd/m² is 58% of the signal, 100 cd/m² about 51%.
        assert!(
            close(pq_encode(203.0), 0.5807, 5e-4),
            "{}",
            pq_encode(203.0)
        );
        assert!(
            close(pq_encode(100.0), 0.5081, 5e-4),
            "{}",
            pq_encode(100.0)
        );
        assert!(
            close(pq_encode(1000.0), 0.7518, 5e-4),
            "{}",
            pq_encode(1000.0)
        );
    }

    #[test]
    fn pq_decodes_what_it_encodes() {
        for nits in [0.01, 0.5, 80.0, 203.0, 600.0, 4000.0, 10_000.0] {
            let back = pq_decode(pq_encode(nits));
            assert!(close(back, nits, nits * 1e-9 + 1e-9), "{nits} -> {back}");
        }
    }

    #[test]
    fn the_primaries_matrix_keeps_white_and_greys() {
        for row in SRGB_TO_BT2020 {
            assert!(close(row.iter().sum::<f64>(), 1.0, 1e-9));
        }
        // sRGB red is inside BT.2020: mostly red, a little of the others.
        let red = SRGB_TO_BT2020.map(|row| row[0]);
        assert!(red[0] > 0.6 && red[1] > 0.0 && red[2] > 0.0);
    }

    #[test]
    fn sdr_white_lands_on_the_reference_white() {
        let white = encode_pq([1.0; 3], 203.0);
        for channel in white {
            assert!(close(channel, pq_encode(203.0), 1e-9));
        }
        // Twice as bright as SDR white is twice the reference.
        let brighter = encode_pq([2.0; 3], 203.0);
        assert!(close(pq_decode(brighter[0]), 406.0, 1e-6));
        let moved = encode_pq([1.0; 3], 300.0);
        assert!(close(pq_decode(moved[1]), 300.0, 1e-6));
    }

    #[test]
    fn the_request_reads_on_and_a_white() {
        assert!(!HdrRequest::parse(None, None).enabled);
        assert!(HdrRequest::parse(Some("1"), None).enabled);
        assert!(!HdrRequest::parse(Some("0"), None).enabled);
        assert!(!HdrRequest::parse(Some("1"), None).prefer_pq);
        let pq = HdrRequest::parse(Some("pq"), None);
        assert!(pq.enabled && pq.prefer_pq);
        assert_eq!(
            HdrRequest::parse(Some("1"), Some("300")).reference_white,
            300
        );
        assert_eq!(HdrRequest::parse(Some("1"), Some("5")).reference_white, 80);
        assert_eq!(HdrRequest::parse(Some("1"), Some("x")).reference_white, 203);
    }

    #[test]
    fn the_encoding_follows_the_buffer_and_the_compositor() {
        let on = HdrRequest {
            enabled: true,
            reference_white: 250,
            prefer_pq: false,
        };
        let all = ColorSupport {
            parametric: true,
            luminances: true,
            bt2020: true,
            srgb_primaries: true,
            pq: true,
            ext_linear: true,
        };
        let pq = Encoding::Pq {
            reference_white: 250,
        };
        assert_eq!(choose_encoding(PresentFormat::Ten, all, on), pq);
        assert_eq!(
            choose_encoding(PresentFormat::Half, all, on),
            Encoding::ExtLinear
        );
        assert_eq!(
            choose_encoding(PresentFormat::Eight, all, on),
            Encoding::Srgb
        );
        let off = HdrRequest::default();
        assert_eq!(
            choose_encoding(PresentFormat::Ten, all, off),
            Encoding::Srgb
        );
        let no_linear = ColorSupport {
            ext_linear: false,
            luminances: false,
            ..all
        };
        assert_eq!(
            choose_encoding(PresentFormat::Half, no_linear, on),
            Encoding::Pq {
                reference_white: DEFAULT_REFERENCE_WHITE
            }
        );
        assert_eq!(
            choose_encoding(PresentFormat::Ten, ColorSupport::default(), on),
            Encoding::Srgb
        );
    }
}
