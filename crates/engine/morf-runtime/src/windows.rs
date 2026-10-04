//! What a configuration declares as windows: its own surface's layer
//! settings, and the popups, toplevels and extra layer surfaces it opens --
//! handed to the host to carry out.

use morf_scene::{Behavior, NodeHandle, Value as SceneValue};
use morf_value::region::Region;

/// Edges used to anchor a configured layer surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceAnchors {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl Default for SurfaceAnchors {
    fn default() -> Self {
        Self {
            top: true,
            right: true,
            bottom: false,
            left: true,
        }
    }
}

/// Compositor space reserved along each output edge by a dedicated surface.
///
/// A layer surface can only reserve space on an unambiguously anchored edge, so
/// a frame drawn on all four edges cannot reserve for itself. These thicknesses
/// ask the engine for one zero-size reserver surface per non-zero edge, which is
/// what shrinks the tiling area.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SurfaceReserve {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

impl SurfaceReserve {
    /// Returns whether any edge asks for reserved space.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Returns the requested thickness per edge, in anchor order.
    pub fn edges(&self) -> [(&'static str, u32); 4] {
        [
            ("top", self.top),
            ("right", self.right),
            ("bottom", self.bottom),
            ("left", self.left),
        ]
    }
}

/// Native layer-surface settings assigned by Lua before startup.
///
/// Not `Eq`: a region carries its shape's parameters, and those are floats.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerSurfaceConfig {
    pub namespace: String,
    pub width: u32,
    pub height: u32,
    pub exclusive_zone: i32,
    pub anchors: SurfaceAnchors,
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    pub layer: String,
    pub keyboard_focus: String,
    pub input_regions: Option<Vec<Region>>,
    pub reserve: SurfaceReserve,
    /// Whether the exclusive zone follows the surface's own size on its
    /// anchored edge, rather than being a number the configuration keeps in
    /// step by hand. A bar that grows should push windows with it.
    pub exclusive_auto: bool,
    /// Whether the whole surface is opaque, so the compositor can skip
    /// blending whatever is behind it. False by default, because a bar with a
    /// transparent corner that claims otherwise draws garbage there.
    pub opaque: bool,
    /// Whether this configuration is a session lock rather than a layer:
    /// one surface per output, drawn under the ext-session-lock protocol,
    /// released when the configuration says so. Asked for by the
    /// configuration itself — `morf.surface.session_lock = true` — because
    /// what a file is for is the file's to say, not the command line's.
    pub session_lock: bool,
    /// Whether the configuration keeps running while the compositor offers
    /// no output at all -- every screen switched off, the dock unplugged --
    /// in one runtime with nothing mapped, so its timers, IPC, D-Bus and
    /// processes can still act (light a screen again, say). Off by default:
    /// a configuration that only draws has nothing to do there.
    pub outputless: bool,
    /// Whether a click anywhere else on the output should reach the
    /// configuration, through a blank surface under this one that covers
    /// the output. `None` never asked: the surface is only made when the
    /// configuration sets this at all, since its place in the layer is fixed
    /// at creation; `Some(false)` is made but inert.
    pub backdrop: Option<bool>,
    /// How much the backdrop darkens what it covers while it is awake, 0 to
    /// 1: a phone's shade dims the screen behind it.
    pub backdrop_dim: f64,
    /// The space translucent colours are mixed in: `"linear"` (the default,
    /// linear light) or `"srgb"` (encoded values, as browsers and Qt mix
    /// them).
    pub blend: String,
    /// Subpixel (LCD) text: `"auto"` (the default: where fontconfig or the
    /// output says the stripes run, and only where it is safe -- text drawn
    /// straight onto an opaque rectangle), `"off"`, or `"rgb"`/`"bgr"` to
    /// name the order outright.
    pub subpixel_text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PopupConstraintConfig {
    pub slide_x: bool,
    pub slide_y: bool,
    pub flip_x: bool,
    pub flip_y: bool,
    pub resize_x: bool,
    pub resize_y: bool,
}

impl Default for PopupConstraintConfig {
    fn default() -> Self {
        Self {
            slide_x: true,
            slide_y: true,
            flip_x: true,
            flip_y: true,
            resize_x: false,
            resize_y: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PopupSurfaceConfig {
    pub parent: Option<u64>,
    pub anchor_x: i32,
    pub anchor_y: i32,
    pub anchor_width: i32,
    pub anchor_height: i32,
    pub width: u32,
    pub height: u32,
    pub anchor_edge: String,
    pub gravity: String,
    pub offset_x: i32,
    pub offset_y: i32,
    pub constraints: PopupConstraintConfig,
    pub grab_focus: bool,
    /// The space translucent colours are mixed in: `"linear"` (the default,
    /// linear light) or `"srgb"` (encoded values, as browsers and Qt mix
    /// them).
    pub blend: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToplevelSurfaceConfig {
    pub parent: Option<u64>,
    pub width: u32,
    pub height: u32,
    pub minimum_width: u32,
    pub minimum_height: u32,
    pub maximum_width: Option<u32>,
    pub maximum_height: Option<u32>,
    pub title: String,
    pub app_id: String,
    pub minimized: bool,
    pub maximized: bool,
    pub fullscreen: bool,
    /// The space translucent colours are mixed in: `"linear"` (the default,
    /// linear light) or `"srgb"` (encoded values, as browsers and Qt mix
    /// them).
    pub blend: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WindowSurfaceKind {
    Popup(PopupSurfaceConfig),
    Toplevel(ToplevelSurfaceConfig),
    /// One additional wlr-layer-shell surface beyond the shell's own.
    Layer(LayerSurfaceConfig),
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowSurfaceConfig {
    pub id: u64,
    pub root: NodeHandle,
    pub visible: bool,
    pub updates_enabled: bool,
    pub kind: WindowSurfaceKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WindowSurfaceAction {
    Move { id: u64 },
    Resize { id: u64, edge: String },
}

impl Default for LayerSurfaceConfig {
    fn default() -> Self {
        Self {
            namespace: "morf".to_owned(),
            width: 0,
            height: 32,
            exclusive_zone: 32,
            anchors: SurfaceAnchors::default(),
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
            layer: "top".to_owned(),
            keyboard_focus: "on_demand".to_owned(),
            input_regions: None,
            reserve: SurfaceReserve::default(),
            exclusive_auto: false,
            opaque: false,
            session_lock: false,
            outputless: false,
            backdrop: None,
            backdrop_dim: 0.0,
            blend: "linear".to_owned(),
            subpixel_text: "auto".to_owned(),
        }
    }
}

/// Deferred parent and anchor transition requested by Lua.
#[derive(Clone, Debug)]
pub struct ParentTransitionRequest {
    pub node: NodeHandle,
    pub parent: NodeHandle,
    pub anchors: Option<std::collections::BTreeMap<String, SceneValue>>,
    pub behavior: Behavior,
}

/// The things a window hears.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowEvent {
    Resized,
    CloseRequested,
    Closed,
    /// The keyboard came to the surface, or left it.
    FocusChanged,
    /// The pointer came over the surface, or left it.
    PointerChanged,
}

impl WindowEvent {
    pub const ALL: [Self; 5] = [
        Self::Resized,
        Self::CloseRequested,
        Self::Closed,
        Self::FocusChanged,
        Self::PointerChanged,
    ];

    /// The method that sets it, and the constructor key.
    pub fn method(self) -> &'static str {
        match self {
            Self::Resized => "on_resize",
            Self::CloseRequested => "on_close_requested",
            Self::Closed => "on_closed",
            Self::FocusChanged => "on_focus_changed",
            Self::PointerChanged => "on_pointer_changed",
        }
    }

    /// Whether a layer surface hears it, as popups and floating windows do.
    pub fn for_layers(self) -> bool {
        matches!(self, Self::FocusChanged | Self::PointerChanged)
    }
}

/// One window's configured size and the signals its reads track.
#[derive(Clone, Copy, Debug)]
pub struct WindowSize {
    pub width: morf_scene::reactive::SignalId,
    pub height: morf_scene::reactive::SignalId,
    pub size: (u32, u32),
}

/// A popup placed against a node of its parent's tree, kept so the popup
/// follows the node as it moves.
#[derive(Clone, Debug)]
pub struct PopupNodeAnchor {
    pub node: NodeHandle,
    pub x: i32,
    pub y: i32,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
}

mod declarations;

pub use declarations::Declarations;
