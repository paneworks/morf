//! The kinds of window a shell opens: layer surfaces (a bar, a dock, an
//! overlay) and toplevels, with what each is configured by.

/// Edges used to anchor a layer-shell surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayerAnchors {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl Default for LayerAnchors {
    fn default() -> Self {
        Self {
            top: true,
            right: true,
            bottom: false,
            left: true,
        }
    }
}

/// Compositor layer used by a layer-shell surface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ShellLayer {
    Background,
    Bottom,
    #[default]
    Top,
    Overlay,
}

/// Keyboard focus policy for a layer-shell surface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum KeyboardFocus {
    None,
    Exclusive,
    #[default]
    OnDemand,
}

/// Configuration for a layer-shell surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerConfig {
    /// Surface namespace exposed to the compositor.
    pub namespace: String,
    /// Requested logical width, or zero for compositor-selected width.
    pub width: u32,
    /// Requested logical height.
    pub height: u32,
    /// Layer-shell exclusive zone in logical pixels.
    pub exclusive_zone: i32,
    /// Compositor output name, or all outputs when unset.
    pub output: Option<String>,
    /// Surface edges anchored to the output.
    pub anchors: LayerAnchors,
    /// Logical top margin.
    pub margin_top: i32,
    /// Logical right margin.
    pub margin_right: i32,
    /// Logical bottom margin.
    pub margin_bottom: i32,
    /// Logical left margin.
    pub margin_left: i32,
    /// Compositor layer used by the surface.
    pub layer: ShellLayer,
    /// Keyboard focus policy used by the surface.
    pub keyboard_focus: KeyboardFocus,
}

impl Default for LayerConfig {
    fn default() -> Self {
        Self {
            namespace: "morf".to_owned(),
            width: 0,
            height: 32,
            exclusive_zone: 32,
            output: None,
            anchors: LayerAnchors::default(),
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
            layer: ShellLayer::default(),
            keyboard_focus: KeyboardFocus::default(),
        }
    }
}

/// Geometry and identity for an xdg toplevel surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToplevelConfig {
    /// Initial logical width.
    pub width: u32,
    /// Initial logical height.
    pub height: u32,
    /// Smallest compositor-configured logical width.
    pub minimum_width: u32,
    /// Smallest compositor-configured logical height.
    pub minimum_height: u32,
    /// Largest compositor-configured logical width when bounded.
    pub maximum_width: Option<u32>,
    /// Largest compositor-configured logical height when bounded.
    pub maximum_height: Option<u32>,
    /// Compositor-visible title.
    pub title: String,
    /// Desktop application identifier.
    pub app_id: String,
    /// Requests initial minimized state.
    pub minimized: bool,
    /// Requests initial maximized state.
    pub maximized: bool,
    /// Requests initial fullscreen state.
    pub fullscreen: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}
