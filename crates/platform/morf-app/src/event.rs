//! Everything a backend tells the host, as one event type.
use crate::backend::wayland::ScreencopyFrame;
use crate::{DropInfo, InputMethodState, KeyModifiers, OfferInfo, ScreenInfo, SurfaceRole, TextInputState};

/// Event produced by the layer-surface connection.
#[derive(Clone, Debug, PartialEq)]
pub enum LayerEvent {
    /// A popup or floating window's own scale changed.
    ///
    /// Separate from `Scale`, which is a layer surface's, because the two are
    /// addressed differently and a caller resizes different things for each.
    AuxScale { role: SurfaceRole, scale_120: u32 },
    /// The compositor granted, or withdrew, the shell's hold on its shortcuts.
    ShortcutsInhibited { active: bool },
    /// The compositor selected a logical size for one layer surface.
    Configure { id: u64, width: u32, height: u32 },
    /// One layer surface's preferred scale changed in protocol-native 120ths.
    Scale { id: u64, scale_120: u32 },
    /// The compositor permits the next animation and paint tick.
    Frame { id: u64, time_ms: u32 },
    /// The pointer moved over or entered the surface.
    PointerMotion {
        surface: SurfaceRole,
        x: f64,
        y: f64,
    },
    /// The pointer left the surface.
    PointerLeave { surface: SurfaceRole },
    /// A pointer button changed state.
    PointerButton {
        surface: SurfaceRole,
        button: u32,
        pressed: bool,
        x: f64,
        y: f64,
        /// The modifiers held when it did: a Shift-click, a Ctrl-click.
        modifiers: KeyModifiers,
    },
    /// A pointer wheel or touchpad axis changed.
    PointerAxis {
        surface: SurfaceRole,
        x: f64,
        y: f64,
        horizontal: f64,
        vertical: f64,
        horizontal_steps: i32,
        vertical_steps: i32,
        /// The modifiers held: Ctrl with the wheel zooms, as a rule.
        modifiers: KeyModifiers,
    },
    /// A touch contact began on the surface.
    TouchDown {
        surface: SurfaceRole,
        id: i32,
        x: f64,
        y: f64,
    },
    /// A touch contact moved on the surface.
    TouchMotion {
        surface: SurfaceRole,
        id: i32,
        x: f64,
        y: f64,
    },
    /// A touch contact ended on the surface.
    TouchUp {
        surface: SurfaceRole,
        id: i32,
        x: f64,
        y: f64,
    },
    /// The compositor cancelled every active touch contact.
    TouchCancel,
    /// A keyboard key changed state.
    Key {
        surface: SurfaceRole,
        keysym: u32,
        text: Option<String>,
        pressed: bool,
        repeat: bool,
        /// The modifiers held when it did.
        modifiers: KeyModifiers,
    },
    /// A configured seat idle threshold changed state.
    Idle {
        timeout_ms: u32,
        /// Whether this threshold counts input only, ignoring idle inhibitors.
        input_only: bool,
        idle: bool,
    },
    /// The compositor clipboard selection changed.
    Clipboard { text: Option<String> },
    /// The selection changed, as data control sees it: with no focus needed,
    /// and before anything is read. `offer` is `None` when it was cleared.
    Selection {
        /// Whether this is the primary selection (middle-click paste).
        primary: bool,
        offer: Option<OfferInfo>,
    },
    /// A read asked for with [`LayerClient::read_offer`] finished.
    OfferRead {
        request_id: u64,
        result: Result<Vec<u8>, String>,
    },
    /// A drag from somewhere came over one of this client's surfaces.
    DragEnter {
        surface: SurfaceRole,
        x: f64,
        y: f64,
        offer: OfferInfo,
    },
    /// The drag moved over the surface it entered.
    DragMotion {
        surface: SurfaceRole,
        x: f64,
        y: f64,
    },
    /// The drag left the surface without dropping, or was cancelled.
    DragLeave { surface: SurfaceRole },
    /// The drag was dropped here, and what it carries has been fetched.
    Drop {
        surface: SurfaceRole,
        x: f64,
        y: f64,
        /// Boxed: the largest payload of any event, and events are moved often.
        drop: Box<DropInfo>,
    },
    /// A drag this client started ended: dropped somewhere, or not.
    DragSourceEnded { dropped: bool },
    /// The keyboard came to the primary surface, or left it for elsewhere.
    ///
    /// With on-demand focus, a click anywhere else takes the keyboard away,
    /// which is how a shell learns that the user has moved on without
    /// covering the screen to hear the click.
    KeyboardFocus { active: bool },
    /// The keyboard came to one of this client's surfaces, or left it: any
    /// surface, the primary one included (which also sends `KeyboardFocus`).
    SurfaceKeyboard { surface: SurfaceRole, focused: bool },
    /// The pointer came over one of this client's surfaces, or left it.
    SurfacePointer { surface: SurfaceRole, inside: bool },
    /// A capture asked for on the GPU has been described by its session.
    ///
    /// The compositor has said what size it will produce, which device the
    /// buffer must live on, and which formats and modifiers it will draw into.
    /// Nothing is allocated yet: the renderer answers with a dmabuf through
    /// `attach_capture_dmabuf`, or falls back to shared memory through
    /// `attach_capture_shm`, and the capture continues either way.
    CaptureOffer {
        /// Runtime-local request identifier.
        request_id: u64,
        /// Pixel width the compositor will produce.
        width: u32,
        /// Pixel height the compositor will produce.
        height: u32,
        /// The `dev_t` of the device the buffer must be allocated on, when
        /// the compositor named one.
        device: Option<u64>,
        /// DRM fourcc codes and, for each, the modifiers the compositor can
        /// draw with, in its order of preference.
        formats: Vec<(u32, Vec<u64>)>,
    },
    /// An output capture completed or failed.
    Screencopy {
        /// Runtime-local request identifier.
        request_id: u64,
        /// Captured pixels or compositor failure.
        result: Result<ScreencopyFrame, String>,
    },
    /// A focused text input committed a new input-method context.
    InputMethod(InputMethodState),
    /// An input method committed edits for this client's text input.
    TextInput(TextInputState),
    /// The compositor output set changed.
    Screens(Vec<ScreenInfo>),
    /// The compositor positioned and sized the popup.
    PopupConfigure { id: u64, width: u32, height: u32 },
    /// The compositor permits the next popup paint tick.
    PopupFrame { id: u64, time_ms: u32 },
    /// The compositor dismissed the popup.
    PopupDone { id: u64 },
    /// The compositor configured the floating window.
    FloatingConfigure { id: u64, width: u32, height: u32 },
    /// The compositor permits the next floating-window paint tick.
    FloatingFrame { id: u64, time_ms: u32 },
    /// The compositor requested that the floating window close.
    FloatingClose { id: u64 },
    /// The compositor accepted exclusive session ownership.
    SessionLocked,
    /// The compositor rejected or ended the session lock.
    SessionLockFinished,
    /// One output lock surface received its logical size.
    SessionLockConfigure {
        index: usize,
        width: u32,
        height: u32,
    },
    /// One output and its lock surface were removed.
    SessionLockSurfaceRemoved { index: usize },
    /// The compositor permits the next lock-surface paint tick.
    SessionLockFrame { index: usize, time_ms: u32 },
    /// The compositor closed one layer surface.
    Closed { id: u64 },
}
