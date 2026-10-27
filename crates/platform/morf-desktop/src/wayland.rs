//! The compositor's desktop protocols behind the [`Desktop`] trait.

use std::sync::Arc;

use morf_app::DesktopLink;

use crate::{
    CaptureBuffer, Desktop, DesktopCapabilities, DesktopEvent, IdleTimeout, OutputPowerMode,
    ToplevelAction, ToplevelInfo, WaylandDesktop,
};

/// Binds the desktop protocols the compositor behind `link` offers; a
/// request naming no output is for the one the link's surface sits on.
pub fn connect(link: DesktopLink) -> Result<Box<dyn Desktop>, String> {
    let mut desktop = WaylandDesktop::new(link.connection)?;
    desktop.set_own_output(link.own_output);
    desktop.set_shell_surface(link.shell_surface);
    Ok(Box::new(desktop))
}

impl Desktop for WaylandDesktop {
    fn capabilities(&self) -> DesktopCapabilities {
        DesktopCapabilities {
            data_control: self.supports_data_control(),
            primary_selection: self.supports_primary_selection(),
            screencopy: self.supports_screencopy(),
            image_capture: self.supports_image_capture(),
            window_capture: self.supports_window_capture(),
            dmabuf_capture: self.supports_dmabuf_capture(),
            idle_notify: self.supports_idle_notify(),
            output_power: self.supports_output_power(),
            toplevels: self.supports_toplevels(),
            toplevel_control: self.supports_toplevel_control(),
        }
    }

    fn dispatch_pending(&mut self) -> Result<(), String> {
        WaylandDesktop::dispatch_pending(self)
    }

    fn next_event(&mut self) -> Option<DesktopEvent> {
        WaylandDesktop::next_event(self)
    }

    fn set_waker(&mut self, waker: morf_app::transfer::Wakeup) {
        WaylandDesktop::set_waker(self, waker);
    }

    fn set_selection(&mut self, data: Vec<(String, Arc<Vec<u8>>)>, primary: bool) -> bool {
        WaylandDesktop::set_selection(self, data, primary)
    }

    fn clear_selection(&mut self, primary: bool) -> bool {
        WaylandDesktop::clear_selection(self, primary)
    }

    fn owns_offer(&self, offer_id: u64) -> bool {
        WaylandDesktop::owns_offer(self, offer_id)
    }

    fn read_offer(&mut self, request_id: u64, offer_id: u64, mime: &str) {
        WaylandDesktop::read_offer(self, request_id, offer_id, mime);
    }

    fn has_output(&self, name: &str) -> bool {
        WaylandDesktop::has_output(self, name)
    }

    fn capture_output(
        &mut self,
        request_id: u64,
        include_cursor: bool,
        output: Option<&str>,
    ) -> bool {
        WaylandDesktop::capture_output(self, request_id, include_cursor, output)
    }

    fn capture_output_image(&mut self, request_id: u64, gpu: bool, output: Option<&str>) -> bool {
        WaylandDesktop::capture_output_image(self, request_id, gpu, output)
    }

    fn capture_window(&mut self, request_id: u64, identifier: &str, gpu: bool) -> bool {
        WaylandDesktop::capture_window(self, request_id, identifier, gpu)
    }

    fn attach_capture_dmabuf(
        &mut self,
        request_id: u64,
        buffer: &CaptureBuffer<'_>,
    ) -> Result<(), String> {
        WaylandDesktop::attach_capture_dmabuf(self, request_id, buffer)
    }

    fn attach_capture_shm(&mut self, request_id: u64) -> bool {
        WaylandDesktop::attach_capture_shm(self, request_id)
    }

    fn set_idle_timeouts(&mut self, timeouts: &[IdleTimeout]) -> bool {
        WaylandDesktop::set_idle_timeouts(self, timeouts)
    }

    fn update_idle_timeouts(&mut self, timeouts: &[IdleTimeout]) -> bool {
        WaylandDesktop::update_idle_timeouts(self, timeouts)
    }

    fn set_output_power(&mut self, mode: OutputPowerMode) -> bool {
        WaylandDesktop::set_output_power(self, mode)
    }

    fn toplevels(&self) -> Vec<ToplevelInfo> {
        WaylandDesktop::toplevels(self)
    }

    fn take_toplevels_changed(&mut self) -> bool {
        WaylandDesktop::take_toplevels_changed(self)
    }

    fn control_toplevel(&mut self, identifier: &str, action: ToplevelAction) -> bool {
        WaylandDesktop::control_toplevel(self, identifier, action)
    }
}
