//! The outputs a window can be put on.

/// Capability-derived compositor output description.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Output {
    pub id: u32,
    pub name: Option<String>,
    pub make: String,
    pub model: String,
    pub description: Option<String>,
    pub position: Option<(i32, i32)>,
    pub size: Option<(i32, i32)>,
    pub physical_size: Option<(i32, i32)>,
    /// The current mode in device pixels, turned as the output is shown:
    /// with `physical_size`, how dense the panel is.
    pub pixels: Option<(i32, i32)>,
    pub scale: i32,
    pub transform: &'static str,
    /// How the output's subpixels are laid out, as `wl_output.subpixel`
    /// says: `unknown`, `none`, `horizontal_rgb`, `horizontal_bgr`,
    /// `vertical_rgb` or `vertical_bgr`.
    pub subpixel: &'static str,
}

/// What an output prefers its pictures described as (its preferred image
/// description under `wp_color_management_output_v1`), asked of a backend
/// with [`crate::Backend::output_color`]. Kept apart from [`Output`]: it
/// arrives later and changes on its own, and an output whose description
/// changes has not moved.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OutputColor {
    /// `pq`, `ext_linear`, `gamma22`, `srgb`, `hlg`, ... or `power` for a
    /// plain exponent; empty when it was not said.
    pub transfer: &'static str,
    /// `srgb`, `bt2020`, `display_p3`, ... or `custom` for chromaticities.
    pub primaries: &'static str,
    /// The darkest it shows, in 1/10000 cd/m².
    pub min_luminance: u32,
    /// The brightest it shows, in cd/m².
    pub max_luminance: u32,
    /// Where SDR white sits, in cd/m².
    pub reference_luminance: u32,
}

impl OutputColor {
    /// Whether the output shows more than SDR white: a PQ or HLG signal, or
    /// a peak above the reference.
    pub fn hdr(&self) -> bool {
        matches!(self.transfer, "pq" | "hlg") || self.max_luminance > self.reference_luminance
    }
}
