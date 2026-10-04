use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawWindowHandle,
    WaylandWindowHandle, WindowHandle,
};
use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::data_device_manager::DataDeviceManagerState;
use smithay_client_toolkit::data_device_manager::data_device::DataDevice;
use smithay_client_toolkit::data_device_manager::data_source::CopyPasteSource;
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::registry::RegistryState;
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::session_lock::{SessionLock, SessionLockState, SessionLockSurface};
use smithay_client_toolkit::shell::wlr_layer::LayerShell;
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shell::xdg::popup::Popup;
use smithay_client_toolkit::shell::xdg::window::Window;
use smithay_client_toolkit::shm::Shm;
use smithay_client_toolkit::shm::slot::{Buffer as ShmBuffer, SlotPool};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::ptr::NonNull;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, mpsc};
use std::time::Instant;
use wayland_client::Proxy;
use wayland_client::protocol::wl_subcompositor::WlSubcompositor;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface, wl_touch};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};

use wayland_protocols::wp::cursor_shape::v1::client::{
    wp_cursor_shape_device_v1::WpCursorShapeDeviceV1,
    wp_cursor_shape_manager_v1::WpCursorShapeManagerV1,
};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::WpFractionalScaleV1,
};
use wayland_protocols::wp::idle_inhibit::zv1::client::{
    zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1, zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1,
};
use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::client::{
    zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1,
    zwp_keyboard_shortcuts_inhibitor_v1::ZwpKeyboardShortcutsInhibitorV1,
};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3, zwp_text_input_v3::ZwpTextInputV3,
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};
use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_manager_v2::ZwpInputMethodManagerV2, zwp_input_method_v2::ZwpInputMethodV2,
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};

use crate::backend::wayland::client_data::ReadTag;
use crate::backend::wayland::{client_surface::*, surface_types::*};
use crate::transfer::ReadDone;

/// Owned Wayland display and surface handles for graphics APIs.
#[derive(Clone, Debug)]
pub struct WaylandWindowTarget {
    pub(crate) backend: wayland_backend::client::Backend,
    pub(crate) surface: wl_surface::WlSurface,
}

impl HasDisplayHandle for WaylandWindowTarget {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.backend.display_handle()
    }
}

impl HasWindowHandle for WaylandWindowTarget {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let pointer =
            NonNull::new(self.surface.id().as_ptr().cast()).ok_or(HandleError::Unavailable)?;
        let raw = RawWindowHandle::Wayland(WaylandWindowHandle::new(pointer));
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

/// Fractional scale for a surface that is not a layer surface.
pub(crate) struct AuxSurfaceScale {
    pub(crate) fractional: Option<WpFractionalScaleV1>,
    pub(crate) viewport: Option<WpViewport>,
    /// Scale in 120ths, the protocol's own unit. 120 is 1x, and is what the
    /// compositor is assumed to want until it says otherwise.
    pub(crate) scale_120: u32,
}

impl Drop for AuxSurfaceScale {
    fn drop(&mut self) {
        if let Some(fractional) = self.fractional.take() {
            fractional.destroy();
        }
        if let Some(viewport) = self.viewport.take() {
            viewport.destroy();
        }
    }
}

/// One live wlr-layer-shell surface and the per-surface state the compositor
/// configures independently of every other layer surface this client owns.
pub(crate) struct LayerRecord {
    pub(crate) surface: ShellSurface,
    pub(crate) fractional_scale: Option<WpFractionalScaleV1>,
    pub(crate) viewport: Option<WpViewport>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) scale_120: u32,
    /// Whether this surface should map itself with a blank buffer once the
    /// compositor has configured it.
    pub(crate) wants_blank: bool,
    /// Whether a configure has been acknowledged, which the protocol requires
    /// before any buffer may be attached.
    pub(crate) configured: bool,
    /// This surface's background-effect object, created the first time it asks
    /// for a blurred backdrop. Kept because destroying it clears the region.
    pub(crate) backdrop: Option<ExtBackgroundEffectSurfaceV1>,
    /// Backing store for a surface mapped with a blank buffer.
    ///
    /// A reserver has no renderer, but a layer surface that never attaches a
    /// buffer stays unmapped, and a compositor computes an output's usable area
    /// only from the layer surfaces it actually arranges. Holding the pool and
    /// the buffer here keeps the mapping alive for as long as the surface is.
    pub(crate) blank: Option<(SlotPool, ShmBuffer)>,
    /// The blank pixel's colour, premultiplied BGRA: transparent for a
    /// reserver, a dim for a backdrop.
    pub(crate) blank_color: [u8; 4],
    /// Where the surface asked to be. Layer-shell is told directly and the
    /// compositor places it; without layer-shell this is what places the
    /// subsurface standing in for it (`placement`).
    pub(crate) request: crate::placement::LayerRequest,
    /// When the surface was opened, from `LayerState::layer_sequence`: breaks
    /// stacking ties between subsurfaces on one layer.
    pub(crate) sequence: u64,
    /// The keyboard focus it asks for, and when it last asked. Only read
    /// without layer-shell, where every stand-in shares the toplevel's
    /// keyboard focus and keys go to the latest surface that wants them.
    /// A `Cell` because the paint path changes it through `&LayerClient`.
    pub(crate) keyboard: std::cell::Cell<(KeyboardFocus, u64)>,
    /// A subsurface's position inside the primary, once placed.
    pub(crate) placed: Option<(i32, i32)>,
}

impl Drop for LayerRecord {
    fn drop(&mut self) {
        if let Some(scale) = self.fractional_scale.take() {
            scale.destroy();
        }
        if let Some(viewport) = self.viewport.take() {
            viewport.destroy();
        }
    }
}

pub(crate) struct LayerState {
    /// When each surface's outstanding frame callback was asked for, by
    /// wl_surface id. Cleared when the callback arrives, so an entry that
    /// grows old names a surface the compositor is not showing -- a fallback
    /// toplevel under another in cage, say -- and a present to it under
    /// FIFO would block the whole output thread waiting for that callback.
    pub(crate) frames_outstanding:
        std::cell::RefCell<HashMap<wayland_client::backend::ObjectId, std::time::Instant>>,
    pub(crate) registry: RegistryState,
    pub(crate) compositor: CompositorState,
    pub(crate) outputs: OutputState,
    pub(crate) seats: SeatState,
    pub(crate) xdg_shell: XdgShell,
    /// The layer shell, when the compositor offers one.
    ///
    /// Optional because `wlr-layer-shell` is an extension and kiosk
    /// compositors omit it; see `ShellSurface` for what happens instead.
    pub(crate) layer_shell: Option<LayerShell>,
    pub(crate) layers: HashMap<u64, LayerRecord>,
    /// The subcompositor, for standing layer surfaces in as subsurfaces of
    /// the primary where there is no layer-shell.
    pub(crate) subcompositor: Option<WlSubcompositor>,
    /// A counter for `LayerRecord::sequence` and keyboard requests.
    pub(crate) layer_sequence: std::cell::Cell<u64>,
    /// The order the subsurfaces were last stacked in, bottom first, so they
    /// are restacked only when it changes.
    pub(crate) subsurface_stack: Vec<u64>,
    /// The layer surface each popup was opened against, so a reposition can
    /// add the same subsurface offset its creation did.
    pub(crate) popup_parents: HashMap<u64, WindowId>,
    pub(crate) popups: HashMap<u64, Popup>,
    /// The size of each popup's last configure.
    pub(crate) popup_sizes: HashMap<u64, (u32, u32)>,
    /// Reposition tokens sent to, and echoed back by, each live popup.
    pub(crate) popup_repositions: HashMap<u64, PopupReposition>,
    pub(crate) floatings: HashMap<u64, Window>,
    pub(crate) floating_sizes: HashMap<u64, (u32, u32)>,
    pub(crate) fractional_manager: Option<WpFractionalScaleManagerV1>,
    pub(crate) viewporter: Option<WpViewporter>,
    pub(crate) events: VecDeque<Event>,
    pub(crate) pointer: Option<wl_pointer::WlPointer>,
    pub(crate) pointer_seat: Option<wl_seat::WlSeat>,
    /// The serial of the pointer's latest entry into one of these surfaces,
    /// which is what a cursor shape has to be asked for against.
    pub(crate) pointer_enter_serial: Option<u32>,
    pub(crate) cursor_shape_manager: Option<WpCursorShapeManagerV1>,
    pub(crate) cursor_device: Option<WpCursorShapeDeviceV1>,
    /// The shape last asked for, so the same one is not sent every motion.
    pub(crate) cursor_shape_current: Option<String>,
    pub(crate) keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(crate) touch: Option<wl_touch::WlTouch>,
    pub(crate) touch_points: HashMap<i32, ((f64, f64), WindowId)>,
    pub(crate) keyboard_surface: Option<WindowId>,
    /// The key being held, repeated by the client (`key_repeat`).
    pub(crate) key_repeat: crate::backend::wayland::key_repeat::KeyRepeat,
    pub(crate) idle_inhibit_manager: Option<ZwpIdleInhibitManagerV1>,
    /// Per-surface scale for popups and floating windows.
    ///
    /// Layer surfaces keep theirs in `LayerRecord`; these have no record of
    /// their own, and until this existed they simply borrowed the primary
    /// layer's scale. That is right exactly when a popup is on the same output
    /// as the bar that opened it, and wrong the moment it is not -- which on a
    /// mixed-DPI desk is most of the time.
    pub(crate) aux_scales: HashMap<WindowId, AuxSurfaceScale>,
    /// The live inhibitor, if the shell is currently holding the session awake.
    ///
    /// Its existence *is* the inhibition — the protocol has no "off", only a
    /// destroy — so this is `Some` exactly while the session is being held.
    pub(crate) idle_inhibitor: Option<ZwpIdleInhibitorV1>,
    pub(crate) shortcuts_inhibit_manager: Option<ZwpKeyboardShortcutsInhibitManagerV1>,
    /// While the shell asks the compositor to stop eating its keys, one
    /// inhibitor per surface that can hold the keyboard: the primary layer
    /// and every floating window. Same shape as the idle inhibitor -- each
    /// object's existence is the request -- but per surface, because the
    /// protocol honours an inhibitor only while *its* surface has focus.
    pub(crate) shortcuts_inhibitors: HashMap<WindowId, ZwpKeyboardShortcutsInhibitorV1>,
    pub(crate) shortcuts_inhibit: crate::backend::wayland::inhibit_handlers::ShortcutsInhibit,
    pub(crate) data_device_manager: Option<DataDeviceManagerState>,
    pub(crate) data_devices: Vec<DataDevice>,
    pub(crate) clipboard_source: Option<CopyPasteSource>,
    pub(crate) clipboard_text: String,
    pub(crate) clipboard_tx: mpsc::Sender<Option<String>>,
    pub(crate) clipboard_rx: mpsc::Receiver<Option<String>>,
    pub(crate) clipboard_reads: Arc<AtomicUsize>,
    pub(crate) clipboard_writes: Arc<AtomicUsize>,
    /// The last offer identifier handed out; selections and drags share it.
    pub(crate) next_offer_id: u64,
    /// Where reader threads report, drained by `next_event`.
    pub(crate) read_tx: mpsc::Sender<ReadDone<ReadTag>>,
    pub(crate) read_rx: mpsc::Receiver<ReadDone<ReadTag>>,
    /// Rung by a transfer thread when it finishes, so the loop wakes for it.
    pub(crate) waker: Option<fn()>,
    /// A drag from elsewhere currently over one of these surfaces, or dropped
    /// on one and not yet finished.
    pub(crate) drag: Option<DragState>,
    /// The surface the pointer last pressed on: where a drag out starts.
    pub(crate) pressed_surface: Option<wl_surface::WlSurface>,
    /// A drag this client started, and what it answers with.
    pub(crate) drag_source: Option<OwnedDrag>,
    pub(crate) latest_input_serial: Option<u32>,
    /// The modifiers the keyboard last reported held.
    pub(crate) modifiers: KeyModifiers,
    pub(crate) virtual_keyboard_manager: Option<ZwpVirtualKeyboardManagerV1>,
    pub(crate) virtual_keyboard: Option<ZwpVirtualKeyboardV1>,
    pub(crate) virtual_keyboard_keymap: Option<String>,
    pub(crate) virtual_keyboard_keymap_file: Option<File>,
    pub(crate) virtual_keyboard_clock: Instant,
    pub(crate) input_method_manager: Option<ZwpInputMethodManagerV2>,
    pub(crate) input_method: Option<ZwpInputMethodV2>,
    pub(crate) input_method_pending: InputMethodState,
    pub(crate) input_method_state: InputMethodState,
    pub(crate) text_input_manager: Option<ZwpTextInputManagerV3>,
    pub(crate) text_input: Option<ZwpTextInputV3>,
    pub(crate) text_input_requested: bool,
    pub(crate) text_input_pending: TextInputState,
    pub(crate) output_power_target: Option<wl_output::WlOutput>,
    pub(crate) shm: Option<Shm>,
    /// `ext-background-effect-v1`, when the compositor offers it.
    ///
    /// The blur it asks for happens entirely on the compositor's side: it holds
    /// every window's buffer and is the only thing that can see what is behind
    /// this surface. A client never receives those pixels — it names a region
    /// and paints over the result with alpha.
    pub(crate) background_effect: Option<ExtBackgroundEffectManagerV1>,
    /// Whether the compositor currently advertises the blur capability.
    ///
    /// Sent when the manager is bound and again whenever it changes, so a
    /// compositor may withdraw it at run time — at which point it stops
    /// applying blur even to regions already set.
    pub(crate) blur_capable: bool,
    pub(crate) screens: Vec<Output>,
    pub(crate) session_locks: SessionLockState,
    /// Whether the compositor offers `ext-session-lock`.
    pub(crate) has_session_lock: bool,
    pub(crate) session_lock: Option<SessionLock>,
    pub(crate) lock_surfaces: Vec<LockSurface>,
}

/// A drag from another client, as far as this one has followed it.
pub(crate) struct DragState {
    pub(crate) id: u64,
    pub(crate) surface: WindowId,
    pub(crate) mime_types: Vec<String>,
    pub(crate) x: f64,
    pub(crate) y: f64,
    /// The serial of the `enter`, which every `accept` must quote.
    pub(crate) serial: u32,
    /// What this client last said it would take.
    pub(crate) accepted: Option<String>,
    /// Set by `drop`: from here the offer is read, not accepted.
    pub(crate) dropped: bool,
    /// Set once `finish` went out; nothing more may be read.
    pub(crate) finished: bool,
    /// Set once the drop event went out, so it goes out once.
    pub(crate) announced: bool,
    /// Prefetches still running before the drop is announced.
    pub(crate) awaiting: usize,
    pub(crate) uris: Vec<String>,
    pub(crate) text: Option<String>,
}

/// A drag this client is the source of.
pub(crate) struct OwnedDrag {
    pub(crate) source: smithay_client_toolkit::data_device_manager::data_source::DragSource,
    pub(crate) data: Vec<(String, Arc<Vec<u8>>)>,
}

pub(crate) struct LockSurface {
    pub(crate) surface: SessionLockSurface,
    pub(crate) output: wl_output::WlOutput,
    pub(crate) size: (u32, u32),
    pub(crate) scale: u32,
    /// The first frame, drawn in shared memory before a GPU exists for the
    /// surface, and kept only until the GPU has drawn one of its own.
    pub(crate) primer: Option<(SlotPool, ShmBuffer)>,
}
