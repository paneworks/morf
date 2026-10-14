//! Presenting deeper than eight bits a channel (`MORF_DEPTH=10`).
//!
//! Only precision: what is drawn is still sRGB, presented as sRGB, so a
//! gradient no longer bands but no colour gets brighter or wider than it
//! was. HDR -- PQ or scRGB output and the colour-management protocol that
//! tells a compositor so -- is not done here.
//!
//! A deep surface draws into half-float targets (`target_format`) and
//! presents as whichever deep format the window takes:
//!
//! - `XBGR2101010` for a surface declared opaque: ten bits a channel, with
//!   no alpha to lose -- its two alpha bits would leave a translucent
//!   surface's edges and shadows in four steps;
//! - `ABGR16161616F` for any surface: half floats, full alpha;
//! - eight bits otherwise, as without the setting.
//!
//! `XBGR`, not `XRGB`: wgpu's one ten-bit format is `A2B10G10R10`, which is
//! the DRM `ABGR2101010` family. An `ARGB2101010` buffer would need a view
//! of another channel order, which an exported image cannot be given.

use super::{Space, backend_types::WgpuBackend};
use crate::BlendSpace;

/// How many bits a channel a surface asks to be presented with.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum OutputDepth {
    #[default]
    Eight,
    /// Ten or more, where the window and the GPU can; eight where not.
    Ten,
}

impl OutputDepth {
    /// Reads `"8"` or `"10"`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "8" => Some(Self::Eight),
            "10" => Some(Self::Ten),
            _ => None,
        }
    }

    /// `MORF_DEPTH`: `10` asks for deep output; anything else is eight.
    pub fn from_env() -> Self {
        std::env::var("MORF_DEPTH")
            .ok()
            .and_then(|value| Self::parse(&value))
            .unwrap_or_default()
    }
}

/// What a frame is presented as.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub(crate) enum PresentFormat {
    /// `ARGB8888` buffers, or the swapchain's own eight-bit format.
    #[default]
    Eight,
    /// `XBGR2101010`: ten bits a channel, opaque.
    Ten,
    /// `ABGR16161616F`: half floats with alpha.
    Half,
}

impl PresentFormat {
    pub(crate) const ALL: [Self; 3] = [Self::Eight, Self::Ten, Self::Half];

    /// The format a buffer of this kind is drawn into.
    pub(crate) fn texture_format(self) -> wgpu::TextureFormat {
        match self {
            Self::Eight => wgpu::TextureFormat::Bgra8Unorm,
            Self::Ten => wgpu::TextureFormat::Rgb10a2Unorm,
            Self::Half => wgpu::TextureFormat::Rgba16Float,
        }
    }

    /// The DRM format a compositor is handed it as.
    pub(crate) fn fourcc(self) -> u32 {
        use super::dmabuf::{FOURCC_ABGR16161616F, FOURCC_ARGB8888, FOURCC_XBGR2101010};
        match self {
            Self::Eight => FOURCC_ARGB8888,
            Self::Ten => FOURCC_XBGR2101010,
            Self::Half => FOURCC_ABGR16161616F,
        }
    }

    pub(crate) fn deep(self) -> bool {
        self != Self::Eight
    }
}

/// The format to present as: the deepest `takes` allows that suits the
/// surface, or eight bits.
pub(crate) fn choose_present(
    depth: OutputDepth,
    opaque: bool,
    takes: impl Fn(PresentFormat) -> bool,
) -> PresentFormat {
    let deep: &[PresentFormat] = match (depth, opaque) {
        (OutputDepth::Eight, _) => &[],
        (OutputDepth::Ten, true) => &[PresentFormat::Ten, PresentFormat::Half],
        (OutputDepth::Ten, false) => &[PresentFormat::Half],
    };
    deep.iter()
        .copied()
        .find(|format| takes(*format))
        .unwrap_or(PresentFormat::Eight)
}

/// What the composite does to the target's values on their way to `output`:
/// `0` copies them, `1` encodes linear light as sRGB, `2` decodes sRGB.
///
/// An eight-bit target is read through a view that already matches the
/// output. A deep target holds what its space writes -- linear light, or
/// encoded values -- with no sRGB view to read it through, so the
/// composite converts whenever that differs from what the output stores.
pub(crate) fn transfer(space: Space, output: wgpu::TextureFormat) -> f64 {
    match (space.deep, output.is_srgb(), space.blend) {
        (false, _, _) => 0.0,
        (true, false, BlendSpace::Linear) => 1.0,
        (true, true, BlendSpace::Srgb) => 2.0,
        (true, _, _) => 0.0,
    }
}

impl WgpuBackend {
    /// Asks for frames `depth` bits a channel deep, where the window and the
    /// GPU can present them. Returns whether what is presented changed; the
    /// next frame is then drawn in full.
    pub fn set_output_depth(&mut self, depth: OutputDepth) -> bool {
        self.depth = depth;
        let changed = self.select_depth();
        if !changed && depth == OutputDepth::Ten {
            self.say_depth();
        }
        changed
    }

    /// Says what a surface asking for ten bits is presented as.
    fn say_depth(&self) {
        eprintln!(
            "morf: gpu: output depth 10 asked; presenting as {} ({})",
            match self.present {
                PresentFormat::Eight => "8-bit",
                PresentFormat::Ten => "XBGR2101010",
                PresentFormat::Half => "ABGR16161616F",
            },
            match (&self.buffers, &self.surface) {
                (Some(_), _) => "own buffers",
                (None, Some(_)) => "swapchain",
                (None, None) => "offscreen",
            }
        );
    }

    /// The depth frames are presented at now, which is eight wherever ten
    /// was asked for and could not be had.
    pub fn output_depth(&self) -> OutputDepth {
        if self.present.deep() {
            OutputDepth::Ten
        } else {
            OutputDepth::Eight
        }
    }

    /// The composite's transfer for this space and presentation.
    pub(crate) fn transfer(&self) -> f64 {
        let output = match &self.surface {
            Some(surface) => surface.config.format,
            None => self.present.texture_format(),
        };
        transfer(self.space, output)
    }

    /// Chooses what to present as from what is asked, what the surface is,
    /// and what the window takes, and rebuilds for it when that changed.
    pub(crate) fn select_depth(&mut self) -> bool {
        let chosen = choose_present(self.depth, self.opaque_surface, |format| {
            match (&self.buffers, &self.surface) {
                (Some(ring), _) => ring.takes(format),
                (None, Some(surface)) => surface.takes(format),
                (None, None) => true,
            }
        });
        let deep = chosen.deep();
        if chosen == self.present && deep == self.space.deep {
            return false;
        }
        self.present = chosen;
        let space = Space {
            blend: self.space.blend,
            deep,
        };
        if !self.set_space(space) {
            self.rebuild_presentation();
        }
        if self.depth == OutputDepth::Ten {
            self.say_depth();
        }
        true
    }

    /// Builds what presents the target again for the format and space in
    /// force: the ring's buffers and composite, or the swapchain's.
    pub(crate) fn rebuild_presentation(&mut self) {
        let transfer_for = |output| transfer(self.space, output);
        if let Some(ring) = &mut self.buffers {
            let output = self.present.texture_format();
            ring.set_format(
                &self.device,
                &self.texture,
                self.present,
                transfer_for(output),
            );
        }
        if let Some(surface) = &mut self.surface {
            let format = surface.format_for(self.present);
            surface.reformat(&self.device, &self.texture, format, transfer_for(format));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_bits_unless_ten_is_asked_and_taken() {
        let all = |_| true;
        let none = |format| format == PresentFormat::Eight;
        assert_eq!(
            choose_present(OutputDepth::Eight, true, all),
            PresentFormat::Eight
        );
        assert_eq!(
            choose_present(OutputDepth::Ten, true, all),
            PresentFormat::Ten
        );
        assert_eq!(
            choose_present(OutputDepth::Ten, false, all),
            PresentFormat::Half
        );
        assert_eq!(
            choose_present(OutputDepth::Ten, true, none),
            PresentFormat::Eight
        );
        assert_eq!(
            choose_present(OutputDepth::Ten, false, none),
            PresentFormat::Eight
        );
    }

    #[test]
    fn a_translucent_surface_never_gets_two_bits_of_alpha() {
        let ten_only = |format| format != PresentFormat::Half;
        assert_eq!(
            choose_present(OutputDepth::Ten, false, ten_only),
            PresentFormat::Eight
        );
        assert_eq!(
            choose_present(OutputDepth::Ten, true, ten_only),
            PresentFormat::Ten
        );
        let half_only = |format| format != PresentFormat::Ten;
        assert_eq!(
            choose_present(OutputDepth::Ten, true, half_only),
            PresentFormat::Half
        );
    }

    #[test]
    fn depth_reads_eight_and_ten_only() {
        assert_eq!(OutputDepth::parse("10"), Some(OutputDepth::Ten));
        assert_eq!(OutputDepth::parse(" 8 "), Some(OutputDepth::Eight));
        assert_eq!(OutputDepth::parse("12"), None);
    }

    #[test]
    fn the_composite_converts_only_what_differs() {
        use wgpu::TextureFormat::{Bgra8UnormSrgb, Rgb10a2Unorm};
        let linear = Space {
            blend: BlendSpace::Linear,
            deep: true,
        };
        let gamma = Space {
            blend: BlendSpace::Srgb,
            deep: true,
        };
        assert_eq!(transfer(linear, Rgb10a2Unorm), 1.0);
        assert_eq!(transfer(gamma, Rgb10a2Unorm), 0.0);
        assert_eq!(transfer(linear, Bgra8UnormSrgb), 0.0);
        assert_eq!(transfer(gamma, Bgra8UnormSrgb), 2.0);
        assert_eq!(transfer(Space::default(), Rgb10a2Unorm), 0.0);
    }

    #[test]
    fn the_formats_are_the_drm_ones_their_textures_store() {
        assert_eq!(PresentFormat::Ten.fourcc(), 0x3033_4258, "XB30");
        assert_eq!(PresentFormat::Half.fourcc(), 0x4834_4241, "AB4H");
        assert_eq!(PresentFormat::ALL.iter().filter(|f| f.deep()).count(), 2);
    }
}
