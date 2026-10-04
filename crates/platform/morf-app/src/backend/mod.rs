//! The backends a window can live on, and the one trait they share.
//!
//! A host asks a backend for windows of every kind a shell needs, hears one
//! [`Event`] type back, and hands each window's [`RenderTarget`] to the
//! renderer. Two backends: Wayland (a compositor) and headless (virtual
//! outputs, a scripted seat and a clock that moves only when told).

use std::time::Duration;

use std::sync::Arc;

use crate::{
    Edge, Event, InputRect, KeyboardFocus, LayerConfig, Output, PopupConfig, ToplevelConfig,
    WindowId,
};

#[cfg(feature = "headless")]
pub mod headless;
#[cfg(feature = "wayland")]
pub mod wayland;

/// The layer surface a shell's main tree is drawn on.
pub const PRIMARY_LAYER: u64 = 0;

/// The buffer size, in device pixels, of a `logical` size at a scale given
/// in 120ths; never zero.
pub fn physical_size(logical: (u32, u32), scale_120: u32) -> (u32, u32) {
    let scale = scale_120.max(1) as u64;
    (
        ((logical.0 as u64 * scale).div_ceil(120)).max(1) as u32,
        ((logical.1 as u64 * scale).div_ceil(120)).max(1) as u32,
    )
}

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
    Popup {
        parent: WindowId,
        config: PopupConfig,
    },
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

    // What follows has a default for a backend without it: headless has no
    // compositor to place a layer, blur behind one, hand over a clipboard or
    // carry text input, so each is off, empty or a no-op there.

    /// Whether `id` is open (configured or not).
    fn has_window(&self, id: WindowId) -> bool {
        self.logical_size(id).is_some()
    }
    /// Declares a rectangle of the window's next buffer changed, in buffer
    /// pixels.
    fn damage(&self, _id: WindowId, _x: i32, _y: i32, _width: i32, _height: i32) {}
    /// The Wayland client underneath, for what only a compositor has (the
    /// desktop protocols on a queue of their own).
    #[cfg(feature = "wayland")]
    fn as_wayland(&self) -> Option<&wayland::LayerClient> {
        None
    }

    fn screens(&self) -> &[Output] {
        self.outputs()
    }
    /// The output this client's windows are on, when there is one.
    fn own_output(&self) -> Option<Output> {
        self.outputs().first().cloned()
    }
    fn surface_scale_120(&self, id: WindowId) -> u32 {
        self.scale_120(id)
    }
    fn primary_logical_size(&self) -> (u32, u32) {
        self.logical_size(WindowId::Layer(PRIMARY_LAYER))
            .unwrap_or((0, 0))
    }
    fn primary_scale_120(&self) -> u32 {
        self.scale_120(WindowId::Layer(PRIMARY_LAYER))
    }
    /// The primary layer's buffer size.
    fn physical_size(&self) -> (u32, u32) {
        physical_size(self.primary_logical_size(), self.primary_scale_120())
    }
    fn layer_logical_size(&self, id: u64) -> Option<(u32, u32)> {
        self.logical_size(WindowId::Layer(id))
    }
    fn layer_scale_120(&self, id: u64) -> Option<u32> {
        self.has_window(WindowId::Layer(id))
            .then(|| self.scale_120(WindowId::Layer(id)))
    }
    fn lock_size(&self, index: usize) -> Option<(u32, u32)> {
        self.logical_size(WindowId::Lock(index))
    }
    fn lock_scale_120(&self, index: usize) -> Option<u32> {
        self.has_window(WindowId::Lock(index))
            .then(|| self.scale_120(WindowId::Lock(index)))
    }
    fn lock_physical_size(&self, index: usize) -> Option<(u32, u32)> {
        Some(physical_size(
            self.lock_size(index)?,
            self.lock_scale_120(index)?,
        ))
    }
    /// How long the layer's last frame callback has been outstanding.
    fn layer_frame_wait(&self, _id: u64) -> Option<Duration> {
        None
    }

    fn supports_layer_shell(&self) -> bool {
        self.capabilities().layer_shell
    }
    fn supports_layer_surfaces(&self) -> bool {
        self.capabilities().layer_surfaces
    }
    fn supports_live_layer_change(&self) -> bool {
        self.capabilities().live_layer_change
    }
    fn supports_backdrop_blur(&self) -> bool {
        self.capabilities().backdrop_blur
    }
    fn supports_drag_and_drop(&self) -> bool {
        self.capabilities().drag_and_drop
    }
    fn supports_text_input(&self) -> bool {
        self.capabilities().text_input
    }
    fn supports_input_method(&self) -> bool {
        self.capabilities().input_method
    }
    fn supports_virtual_keyboard(&self) -> bool {
        false
    }
    fn supports_idle_inhibit(&self) -> bool {
        false
    }
    fn supports_clipboard(&self) -> bool {
        false
    }
    fn can_set_clipboard(&self) -> bool {
        false
    }

    /// Moves, resizes or re-anchors an open layer surface.
    fn set_layer_geometry(&mut self, _id: u64, _config: &LayerConfig) -> Result<(), String> {
        Ok(())
    }
    /// Maps the layer with a blank buffer, ahead of its first frame.
    fn map_layer_blank(&mut self, _id: u64) -> Result<(), String> {
        Ok(())
    }
    fn set_layer_blank_color(&mut self, _id: u64, _alpha: u8) -> Result<(), String> {
        Ok(())
    }
    fn set_layer_opaque(&self, _id: u64, _opaque: bool) {}
    fn set_layer_keyboard_focus(&self, _id: u64, _focus: KeyboardFocus) -> bool {
        false
    }
    /// The layer's input region, composed from `regions` over its size.
    fn set_layer_composed_input_region(
        &self,
        id: u64,
        regions: &[morf_value::region::Region],
    ) -> Result<(), String> {
        let (width, height) = self
            .layer_logical_size(id)
            .ok_or_else(|| "layer surface is not open".to_owned())?;
        let rectangles =
            morf_value::region::build(width, height, regions).map_err(|error| error.to_string())?;
        self.set_input_region(WindowId::Layer(id), Some(&rectangles));
        Ok(())
    }
    /// Where the compositor blurs behind the layer.
    fn set_layer_backdrop_region(
        &self,
        _id: u64,
        _rectangles: Option<&[morf_value::region::Rect]>,
    ) -> Result<(), String> {
        Ok(())
    }
    /// Moves an open popup. `Ok(false)`: it cannot be moved and has to be
    /// reopened.
    fn reposition_popup(&mut self, _id: u64, _config: PopupConfig) -> Result<bool, String> {
        Ok(false)
    }

    fn set_clipboard(&mut self, _text: String) -> bool {
        false
    }
    fn set_idle_inhibited(&mut self, _inhibited: bool) -> bool {
        false
    }
    fn set_shortcuts_inhibited(&mut self, _inhibited: bool) -> bool {
        false
    }
    fn start_drag(&mut self, _data: Vec<(String, Arc<Vec<u8>>)>) -> bool {
        false
    }
    fn accept_drag(&mut self, _mime: Option<&str>) {}
    fn finish_drop(&mut self) {}
    /// Reads a data offer as `mime`; the bytes arrive as an event.
    fn read_offer(&mut self, _request_id: u64, _offer_id: u64, _mime: &str) {}

    fn enable_text_input(&mut self) -> bool {
        false
    }
    fn disable_text_input(&mut self) -> bool {
        false
    }
    fn set_text_input_surrounding(&self, _text: &str, _cursor: i32, _anchor: i32) -> bool {
        false
    }
    fn set_text_input_cursor_rect(&self, _rect: InputRect) -> bool {
        false
    }
    fn set_text_input_content_type(&self, _hints: u32, _purpose: u32) -> bool {
        false
    }
    fn enable_input_method(&mut self) -> bool {
        false
    }
    fn input_method_commit(&self, _text: &str) -> bool {
        false
    }
    fn input_method_preedit(&self, _text: &str, _begin: i32, _end: i32) -> bool {
        false
    }
    fn input_method_delete(&self, _before: u32, _after: u32) -> bool {
        false
    }
    fn send_virtual_key(&mut self, _keycode: u32, _pressed: bool) -> bool {
        false
    }
    fn send_virtual_modifiers(
        &mut self,
        _depressed: u32,
        _latched: u32,
        _locked: u32,
        _group: u32,
    ) -> bool {
        false
    }
}
