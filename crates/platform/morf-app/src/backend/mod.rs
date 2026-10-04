//! The backends a window can live on, and the one trait they share.
//!
//! A host asks a backend for windows of every kind a shell needs, hears one
//! [`Event`] type back, and hands each window's [`RenderTarget`] to the
//! renderer. Two backends: Wayland (a compositor) and headless (virtual
//! outputs, a scripted seat and a clock that moves only when told).

use std::time::Duration;

use crate::{Edge, Event, InputRect, LayerConfig, Output, PopupConfig, ToplevelConfig, WindowId};

#[cfg(feature = "headless")]
pub mod headless;
#[cfg(feature = "wayland")]
pub mod wayland;

/// What a window is, and what it is opened with.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowKind {
    /// A layer surface: a bar, a dock, an overlay, anchored to an output.
    Layer(LayerConfig),
    /// A toplevel, optionally the child of another toplevel.
    Toplevel {
        parent: Option<u64>,
        config: ToplevelConfig,
    },
    /// A popup, placed relative to its parent.
    Popup { parent: WindowId, config: PopupConfig },
}

/// What a backend can do. A shell degrades on what is missing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Capabilities {
    /// Layer surfaces placed by the compositor (not stood in for).
    pub layer_shell: bool,
    /// Layer surfaces at all, the stood-in kind included.
    pub layer_surfaces: bool,
    /// A layer surface's layer changed without reopening it.
    pub live_layer_change: bool,
    pub toplevels: bool,
    pub popups: bool,
    pub session_lock: bool,
    pub drag_and_drop: bool,
    pub text_input: bool,
    pub input_method: bool,
    /// The compositor blurs what is behind a window.
    pub backdrop_blur: bool,
}

/// Where a window's frames go.
#[derive(Clone, Debug)]
pub enum RenderTarget {
    /// A Wayland surface: a swapchain, or the window's own buffer sink.
    #[cfg(feature = "wayland")]
    Wayland(wayland::WaylandWindowTarget),
    /// No window: an offscreen texture of this size.
    Offscreen { width: u32, height: u32 },
}

/// A window system morf's windows live on.
pub trait Backend {
    fn capabilities(&self) -> Capabilities;
    /// The outputs there are now.
    fn outputs(&self) -> &[Output];
    /// Opens `id` as a `kind` window (a window already open as `id` is
    /// replaced). Its size arrives as a configure event.
    fn open(&mut self, id: WindowId, kind: WindowKind) -> Result<(), String>;
    fn close(&mut self, id: WindowId);
    /// The size the backend gave the window, in logical pixels.
    fn logical_size(&self, id: WindowId) -> Option<(u32, u32)>;
    /// The window's scale, in 120ths.
    fn scale_120(&self, id: WindowId) -> u32;
    /// Asks for a frame event when the window may draw again.
    fn request_frame(&self, id: WindowId);
    /// Commits the window's pending state.
    fn commit(&self, id: WindowId);
    /// Where the window takes the pointer: `None` is everywhere.
    fn set_input_region(&self, id: WindowId, region: Option<&[InputRect]>);
    /// Starts an interactive move of a toplevel. Returns whether it began.
    fn start_move(&self, id: WindowId) -> bool;
    /// Starts an interactive resize of a toplevel from `edge`.
    fn start_resize(&self, id: WindowId, edge: Edge) -> bool;
    /// Sets the pointer's shape by its CSS name. Returns whether it could.
    fn set_cursor(&mut self, shape: &str) -> bool;
    /// Locks the session: lock surfaces come, one per output, as events.
    fn lock(&mut self) -> Result<(), String>;
    fn unlock(&mut self) -> Result<(), String>;
    fn render_target(&self, id: WindowId) -> Option<RenderTarget>;
    /// The next event, if one is queued.
    fn next_event(&mut self) -> Option<Event>;
    /// Waits up to `timeout` (forever: `None`) for events. Returns whether
    /// any came.
    fn dispatch(&mut self, timeout: Option<Duration>) -> Result<bool, String>;
}
