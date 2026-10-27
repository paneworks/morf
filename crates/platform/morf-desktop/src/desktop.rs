//! The desktop protocols as one trait object, so a host talks to a
//! compositor's and to a headless stand-in the same way.

use std::sync::Arc;

use crate::{CaptureBuffer, DesktopEvent, OutputPowerMode, ToplevelAction, ToplevelInfo};

/// An idle threshold: milliseconds, and whether it counts input only
/// (ignoring whatever inhibits idleness).
pub type IdleTimeout = (u32, bool);

/// Which desktop protocols are there. A host asks before it acts on one and
/// leaves a request for a missing one unanswered.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DesktopCapabilities {
    /// The selection watched and set without focus.
    pub data_control: bool,
    /// The primary selection too.
    pub primary_selection: bool,
    /// Outputs captured through `wlr-screencopy`.
    pub screencopy: bool,
    /// Outputs captured through `ext-image-copy-capture`.
    pub image_capture: bool,
    /// Single windows captured.
    pub window_capture: bool,
    /// Captures drawn straight into a dmabuf (the protocol side only).
    pub dmabuf_capture: bool,
    pub idle_notify: bool,
    pub output_power: bool,
    /// Other windows listed.
    pub toplevels: bool,
    /// Other windows acted on.
    pub toplevel_control: bool,
}

impl DesktopCapabilities {
    /// Every protocol there.
    pub const ALL: Self = Self {
        data_control: true,
        primary_selection: true,
        screencopy: true,
        image_capture: true,
        window_capture: true,
        dmabuf_capture: true,
        idle_notify: true,
        output_power: true,
        toplevels: true,
        toplevel_control: true,
    };

    /// Whether any output capture path is there.
    pub fn output_capture(&self) -> bool {
        self.screencopy || self.image_capture
    }

    /// The capabilities as `morf.capabilities` names them.
    pub fn named(&self) -> [(&'static str, bool); 10] {
        [
            ("data_control", self.data_control),
            ("primary_selection", self.primary_selection),
            ("screencopy", self.screencopy),
            ("image_capture", self.image_capture),
            ("window_capture", self.window_capture),
            ("dmabuf_capture", self.dmabuf_capture),
            ("idle_notify", self.idle_notify),
            ("output_power", self.output_power),
            ("toplevels", self.toplevels),
            ("toplevel_control", self.toplevel_control),
        ]
    }
}

/// The desktop protocols a shell uses, whatever carries them.
///
/// Every request returns whether it was taken; one for a protocol
/// [`Desktop::capabilities`] reports missing is refused.
pub trait Desktop {
    // The loop.

    /// Which protocols are there now.
    fn capabilities(&self) -> DesktopCapabilities;
    /// Hears what arrived since the last call. The host's loop reads the
    /// socket; this takes what it read.
    fn dispatch_pending(&mut self) -> Result<(), String>;
    /// The next thing the desktop protocols have to tell.
    fn next_event(&mut self) -> Option<DesktopEvent>;
    /// Rings `waker` whenever work finishing on a thread has news.
    fn set_waker(&mut self, waker: morf_app::transfer::Wakeup);

    // The clipboard, over data control.

    /// Owns the selection, offering each type with its bytes.
    fn set_selection(&mut self, data: Vec<(String, Arc<Vec<u8>>)>, primary: bool) -> bool;
    fn clear_selection(&mut self, primary: bool) -> bool;
    /// Whether `offer_id` is one of the selection offers announced here.
    fn owns_offer(&self, offer_id: u64) -> bool;
    /// Reads one type of an announced offer; the answer arrives as
    /// [`DesktopEvent::OfferRead`], failures included.
    fn read_offer(&mut self, request_id: u64, offer_id: u64, mime: &str);

    // Screen capture.

    /// Whether an output of this name is connected.
    fn has_output(&self, name: &str) -> bool;
    /// Captures an output through `wlr-screencopy`, the cursor included
    /// when asked.
    fn capture_output(
        &mut self,
        request_id: u64,
        include_cursor: bool,
        output: Option<&str>,
    ) -> bool;
    /// Captures an output through `ext-image-copy-capture`.
    fn capture_output_image(&mut self, request_id: u64, gpu: bool, output: Option<&str>) -> bool;
    /// Captures one window by its identifier.
    fn capture_window(&mut self, request_id: u64, identifier: &str, gpu: bool) -> bool;
    /// Answers a [`DesktopEvent::CaptureOffer`] with a dmabuf.
    fn attach_capture_dmabuf(
        &mut self,
        request_id: u64,
        buffer: &CaptureBuffer<'_>,
    ) -> Result<(), String>;
    /// Answers a [`DesktopEvent::CaptureOffer`] with shared memory.
    fn attach_capture_shm(&mut self, request_id: u64) -> bool;

    // Idle and output power.

    /// Replaces the idle thresholds, every clock started again.
    fn set_idle_timeouts(&mut self, timeouts: &[IdleTimeout]) -> bool;
    /// Changes the idle thresholds, keeping the clocks of those still wanted.
    fn update_idle_timeouts(&mut self, timeouts: &[IdleTimeout]) -> bool;
    /// Powers the shell's own output, or every one, on or off.
    fn set_output_power(&mut self, mode: OutputPowerMode) -> bool;

    // Other windows.

    /// Every window fully described.
    fn toplevels(&self) -> Vec<ToplevelInfo>;
    /// Whether the list changed since this was last asked.
    fn take_toplevels_changed(&mut self) -> bool;
    fn control_toplevel(&mut self, identifier: &str, action: ToplevelAction) -> bool;
}
