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
    pub scale: i32,
    pub transform: &'static str,
    /// How the output's subpixels are laid out, as `wl_output.subpixel`
    /// says: `unknown`, `none`, `horizontal_rgb`, `horizontal_bgr`,
    /// `vertical_rgb` or `vertical_bgr`.
    pub subpixel: &'static str,
}

/// Compositor output power state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputPowerMode {
    /// The output is powered down.
    Off,
    /// The output is powered on.
    On,
}
