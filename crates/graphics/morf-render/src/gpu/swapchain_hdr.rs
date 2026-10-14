//! HDR through the swapchain (`MORF_PRESENT=swapchain MORF_HDR=1`).
//!
//! The swapchain belongs to the driver, and on Wayland so does its surface's
//! colour description: Mesa's WSI sets `wp_color_management_surface_v1` from
//! the swapchain's colour space, and a second description from morf on the
//! same surface would be a protocol error. So here HDR is a colour space the
//! surface offers for the deep format -- `Bt2100Pq` (PQ over BT.2020) or
//! `ExtendedSrgbLinear` (scRGB) -- and the composite encodes for it as it
//! does for morf's own buffers. A surface offering neither stays sRGB.

use morf_value::present::Encoding;
use wgpu::{SurfaceColorSpace, SurfaceColorSpaces, TextureFormat};

use super::depth::PresentFormat;
use super::hdr::DEFAULT_REFERENCE_WHITE;
use super::targets::SurfaceState;

/// The HDR encoding a swapchain presenting as `format` can be configured
/// for, given the colour spaces it offers each texture format.
pub(crate) fn choose(
    format: PresentFormat,
    offered: &[(TextureFormat, SurfaceColorSpaces)],
) -> Encoding {
    let spaces = offered
        .iter()
        .find(|(each, _)| *each == format.texture_format())
        .map_or(SurfaceColorSpaces::empty(), |(_, spaces)| *spaces);
    let pq = Encoding::Pq {
        // The driver's description keeps PQ's own reference white.
        reference_white: DEFAULT_REFERENCE_WHITE,
    };
    match format {
        PresentFormat::Half if spaces.contains(SurfaceColorSpaces::EXTENDED_SRGB_LINEAR) => {
            Encoding::ExtLinear
        }
        PresentFormat::Ten | PresentFormat::Half
            if spaces.contains(SurfaceColorSpaces::BT2100_PQ) =>
        {
            pq
        }
        _ => Encoding::Srgb,
    }
}

/// The colour space a swapchain is configured with for `encoding`.
pub(crate) fn color_space(encoding: Encoding) -> SurfaceColorSpace {
    match encoding {
        Encoding::Srgb => SurfaceColorSpace::Auto,
        Encoding::Pq { .. } => SurfaceColorSpace::Bt2100Pq,
        Encoding::ExtLinear => SurfaceColorSpace::ExtendedSrgbLinear,
    }
}

impl SurfaceState {
    /// The HDR encoding this swapchain can present `format` in.
    pub(crate) fn hdr_encoding(&self, format: PresentFormat) -> Encoding {
        choose(format, &self.spaces)
    }

    /// What the swapchain offers, for the log.
    pub(crate) fn offered(&self) -> String {
        let deep = [TextureFormat::Rgb10a2Unorm, TextureFormat::Rgba16Float];
        let listed: Vec<String> = self
            .spaces
            .iter()
            .filter(|(format, _)| deep.contains(format))
            .map(|(format, spaces)| format!("{format:?} {spaces:?}"))
            .collect();
        format!("swapchain colour spaces: {}", listed.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_swapchain_takes_the_hdr_space_its_format_offers() {
        let both = SurfaceColorSpaces::SRGB
            | SurfaceColorSpaces::BT2100_PQ
            | SurfaceColorSpaces::EXTENDED_SRGB_LINEAR;
        let offered = [
            (TextureFormat::Rgb10a2Unorm, both),
            (TextureFormat::Rgba16Float, both),
        ];
        let pq = Encoding::Pq {
            reference_white: DEFAULT_REFERENCE_WHITE,
        };
        assert_eq!(choose(PresentFormat::Ten, &offered), pq);
        assert_eq!(choose(PresentFormat::Half, &offered), Encoding::ExtLinear);
        assert_eq!(choose(PresentFormat::Eight, &offered), Encoding::Srgb);
        let pq_only = [(TextureFormat::Rgba16Float, SurfaceColorSpaces::BT2100_PQ)];
        assert_eq!(choose(PresentFormat::Half, &pq_only), pq);
        let srgb = [(TextureFormat::Rgb10a2Unorm, SurfaceColorSpaces::SRGB)];
        assert_eq!(choose(PresentFormat::Ten, &srgb), Encoding::Srgb);
        assert_eq!(color_space(pq), SurfaceColorSpace::Bt2100Pq);
        assert_eq!(color_space(Encoding::Srgb), SurfaceColorSpace::Auto);
    }
}
