//! Capturing outputs and windows: `ext-image-copy-capture` (outputs and,
//! through the foreign-toplevel source, single windows; into shared memory
//! or a dmabuf the renderer allocates) and `wlr-screencopy` (outputs, where
//! the newer protocol is missing).

mod dmabuf;
mod image_copy;
mod screencopy;

use smithay_client_toolkit::shm::slot::{Buffer as ShmBuffer, SlotPool};
use wayland_client::protocol::{wl_buffer, wl_output, wl_shm};
use wayland_protocols::ext::image_capture_source::v1::client::ext_image_capture_source_v1::ExtImageCaptureSourceV1;
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1,
    ext_image_copy_capture_manager_v1::{self, ExtImageCopyCaptureManagerV1},
    ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1;

use crate::{Desktop, DesktopState};

/// Pixel encoding returned by a compositor screencopy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreencopyFormat {
    Argb8888,
    Xrgb8888,
}


/// One completed output capture in row-major shared-memory layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScreencopyFrame {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Bytes between adjacent rows.
    pub stride: u32,
    /// Pixel channel encoding.
    pub format: ScreencopyFormat,
    /// Whether rows are ordered bottom-to-top.
    pub y_invert: bool,
    /// Captured bytes including stride padding.
    ///
    /// Empty when `dmabuf` is set: the picture is then in the buffer that was
    /// attached for this capture, on the GPU, and was never copied out.
    pub pixels: Vec<u8>,
    /// Whether the compositor drew into the attached dmabuf rather than into
    /// shared memory.
    pub dmabuf: bool,
}


/// A dmabuf to capture into, described the way `zwp_linux_dmabuf_v1` wants it.
///
/// One plane, because that is what the capture protocol carries; the renderer
/// that exported it says where the plane starts and how wide a row is, and
/// which modifier the driver laid it out with -- which the compositor needs
/// to read the memory the same way.
#[derive(Debug)]
pub struct CaptureBuffer<'a> {
    /// The dmabuf's file descriptor, borrowed for the duration of the call.
    pub fd: std::os::fd::BorrowedFd<'a>,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// DRM fourcc of the pixels.
    pub fourcc: u32,
    /// DRM format modifier the memory is laid out with.
    pub modifier: u64,
    /// Byte offset of the plane within the dmabuf.
    pub offset: u32,
    /// Bytes between adjacent rows.
    pub stride: u32,
}


pub(crate) struct PendingScreencopy {
    pub(crate) request_id: u64,
    pub(crate) frame: ZwlrScreencopyFrameV1,
    pub(crate) offer: Option<(wl_shm::Format, u32, u32, u32)>,
    pub(crate) pool: Option<SlotPool>,
    pub(crate) buffer: Option<ShmBuffer>,
    pub(crate) format: Option<ScreencopyFormat>,
    pub(crate) y_invert: bool,
}


/// One capture in flight on `ext-image-copy-capture-v1`.
///
/// More states than the older protocol needed, because this one negotiates
/// before it copies: the session reports the size and formats it can produce,
/// and only once that is `done` is there anything to allocate a buffer against.
/// A frame is then created, given the buffer, and told to capture.
pub(crate) struct PendingCapture {
    pub(crate) request_id: u64,
    pub(crate) session: ExtImageCopyCaptureSessionV1,
    pub(crate) frame: Option<ExtImageCopyCaptureFrameV1>,
    /// Size the session says it will produce, from `buffer_size`.
    pub(crate) size: Option<(u32, u32)>,
    /// The first shared-memory format offered that this engine can carry.
    pub(crate) format: Option<wl_shm::Format>,
    pub(crate) pool: Option<SlotPool>,
    pub(crate) buffer: Option<ShmBuffer>,
    /// Whether the frame has been created and told to capture.
    pub(crate) started: bool,
    /// Whether the configuration asked for the picture on the GPU.
    pub(crate) gpu: bool,
    /// Whether a `CaptureOffer` has gone out and is awaiting a buffer.
    pub(crate) offered: bool,
    /// The device the compositor wants the dmabuf on, from `dmabuf_device`.
    pub(crate) dmabuf_device: Option<u64>,
    /// Every dmabuf format the session offered, with its modifiers.
    pub(crate) dmabuf_formats: Vec<(u32, Vec<u64>)>,
    /// The dmabuf `wl_buffer` the frame was given, and its fourcc.
    pub(crate) dmabuf_buffer: Option<(wl_buffer::WlBuffer, u32)>,
}

impl Desktop {
    /// Whether the newer capture protocol is available, with an output source.
    pub fn supports_image_capture(&self) -> bool {
        self.state.capture_manager.is_some() && self.state.output_source_manager.is_some()
    }

    /// Whether a single window can be captured on its own.
    ///
    /// Separate from the above because a compositor may implement the copy
    /// machinery and only the output source — and the difference is the whole
    /// difference between a screenshot and an overview.
    pub fn supports_window_capture(&self) -> bool {
        self.state.capture_manager.is_some() && self.state.toplevel_source_manager.is_some()
    }

    /// Captures one window, named by the identifier `morf.windows` reported.
    ///
    /// By identifier rather than by index or title: an index means something
    /// different the moment a window opens, and two windows of one application
    /// share a title as readily as an app id.
    ///
    /// With `gpu`, the session's dmabuf offer is reported as a `CaptureOffer`
    /// instead of being answered with shared memory, so the renderer can hand
    /// over a buffer the compositor draws into directly.
    pub fn capture_window(&mut self, request_id: u64, identifier: &str, gpu: bool) -> bool {
        let (Some(manager), Some(sources)) = (
            self.state.capture_manager.clone(),
            self.state.toplevel_source_manager.clone(),
        ) else {
            return false;
        };
        let Some(handle) = self.state.toplevel_handles.get(identifier).cloned() else {
            return false;
        };
        if self.state.shm.is_none() || self.state.captures.len() >= 8 {
            return false;
        }
        let qh = self.queue.handle();
        let source = sources.create_source(&handle, &qh, ());
        self.start_capture(request_id, gpu, &manager, &source, &qh);
        true
    }

    /// Whether an output of this name is connected, as `morf.screens`
    /// reports names.
    pub fn has_output(&self, name: &str) -> bool {
        self.state.output_named(name).is_some()
    }

    /// The output a capture is of: the one named, or else the one this
    /// shell sits on, or else the first there is.
    pub(crate) fn capture_target(&self, name: Option<&str>) -> Option<wl_output::WlOutput> {
        match name.or(self.state.own_output.as_deref()) {
            Some(name) => self.state.output_named(name),
            None => self.state.outputs.outputs().next(),
        }
    }

    /// Captures an output through the newer protocol.
    pub fn capture_output_image(
        &mut self,
        request_id: u64,
        gpu: bool,
        output: Option<&str>,
    ) -> bool {
        let (Some(manager), Some(sources)) = (
            self.state.capture_manager.clone(),
            self.state.output_source_manager.clone(),
        ) else {
            return false;
        };
        let Some(output) = self.capture_target(output) else {
            return false;
        };
        if self.state.shm.is_none() || self.state.captures.len() >= 8 {
            return false;
        }
        let qh = self.queue.handle();
        let source = sources.create_source(&output, &qh, ());
        self.start_capture(request_id, gpu, &manager, &source, &qh);
        true
    }

    /// Opens a session against a source and waits for it to describe itself.
    ///
    /// Nothing is allocated here. The session has not yet said what size or
    /// format it can produce, and guessing would mean allocating a buffer the
    /// compositor is about to refuse.
    fn start_capture(
        &mut self,
        request_id: u64,
        gpu: bool,
        manager: &ExtImageCopyCaptureManagerV1,
        source: &ExtImageCaptureSourceV1,
        qh: &wayland_client::QueueHandle<DesktopState>,
    ) {
        let session = manager.create_session(
            source,
            // Cursors are a separate session in this protocol, and a thumbnail
            // with somebody's pointer baked into it is not a thumbnail of the
            // window.
            ext_image_copy_capture_manager_v1::Options::empty(),
            qh,
            (),
        );
        self.state.captures.push(PendingCapture {
            request_id,
            session,
            frame: None,
            size: None,
            format: None,
            pool: None,
            buffer: None,
            started: false,
            // Only an offer the compositor can honour is worth making: with
            // no dmabuf global there is no way to hand it a buffer.
            gpu: gpu && self.state.linux_dmabuf.is_some(),
            offered: false,
            dmabuf_device: None,
            dmabuf_formats: Vec::new(),
            dmabuf_buffer: None,
        });
    }

    /// Starts an asynchronous capture of the named output, or of the
    /// configured or first one.
    pub fn capture_output(
        &mut self,
        request_id: u64,
        include_cursor: bool,
        output: Option<&str>,
    ) -> bool {
        let Some(output) = self.capture_target(output) else {
            return false;
        };
        let Some(manager) = &self.state.screencopy_manager else {
            return false;
        };
        if self.state.shm.is_none() || self.state.screencopies.len() >= 4 {
            return false;
        }
        let frame =
            manager.capture_output(i32::from(include_cursor), &output, &self.queue.handle(), ());
        self.state.screencopies.push(PendingScreencopy {
            request_id,
            frame,
            offer: None,
            pool: None,
            buffer: None,
            format: None,
            y_invert: false,
        });
        true
    }

    pub fn supports_screencopy(&self) -> bool {
        self.state.screencopy_manager.is_some() && self.state.shm.is_some()
    }
}
