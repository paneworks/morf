//! `wlr-screencopy`: an output copied into shared memory.

use smithay_client_toolkit::shm::slot::SlotPool;
use wayland_client::protocol::wl_shm;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::{
    self, ZwlrScreencopyFrameV1,
};

use super::{ScreencopyFormat, ScreencopyFrame};
use crate::{DesktopEvent, DesktopState};

impl DesktopState {
    pub(crate) fn start_screencopy(&mut self, frame: &ZwlrScreencopyFrameV1) -> Result<(), String> {
        let pending = self
            .screencopies
            .iter_mut()
            .find(|pending| pending.frame == *frame)
            .ok_or_else(|| "unknown screencopy frame".to_owned())?;
        if pending.buffer.is_some() {
            return Ok(());
        }
        let (format, width, height, stride) = pending
            .offer
            .ok_or_else(|| "compositor supplied no shared-memory format".to_owned())?;
        let public_format = match format {
            wl_shm::Format::Argb8888 => ScreencopyFormat::Argb8888,
            wl_shm::Format::Xrgb8888 => ScreencopyFormat::Xrgb8888,
            _ => return Err(format!("unsupported screencopy format {format:?}")),
        };
        if width == 0
            || height == 0
            || stride
                < width
                    .checked_mul(4)
                    .ok_or_else(|| "screencopy width overflow".to_owned())?
        {
            return Err("invalid screencopy dimensions".to_owned());
        }
        let byte_len = (height as usize)
            .checked_mul(stride as usize)
            .filter(|size| *size <= 64 * 1024 * 1024)
            .ok_or_else(|| "screencopy buffer exceeds 64 MiB".to_owned())?;
        let width = i32::try_from(width).map_err(|_| "screencopy width is too large".to_owned())?;
        let height =
            i32::try_from(height).map_err(|_| "screencopy height is too large".to_owned())?;
        let stride =
            i32::try_from(stride).map_err(|_| "screencopy stride is too large".to_owned())?;
        let shm = self
            .shm
            .as_ref()
            .ok_or_else(|| "wl_shm is unavailable".to_owned())?;
        let mut pool = SlotPool::new(byte_len.max(1), shm).map_err(|error| error.to_string())?;
        let (buffer, _) = pool
            .create_buffer(width, height, stride, format)
            .map_err(|error| error.to_string())?;
        frame.copy(buffer.wl_buffer());
        pending.pool = Some(pool);
        pending.buffer = Some(buffer);
        pending.format = Some(public_format);
        Ok(())
    }

    pub(crate) fn finish_screencopy(
        &mut self,
        frame: &ZwlrScreencopyFrameV1,
    ) -> Result<ScreencopyFrame, String> {
        let index = self
            .screencopies
            .iter()
            .position(|pending| pending.frame == *frame)
            .ok_or_else(|| "unknown screencopy frame".to_owned())?;
        let mut pending = self.screencopies.remove(index);
        let mut pool = pending
            .pool
            .take()
            .ok_or_else(|| "screencopy has no shared-memory pool".to_owned())?;
        let buffer = pending
            .buffer
            .take()
            .ok_or_else(|| "screencopy has no shared-memory buffer".to_owned())?;
        let pixels = buffer
            .canvas(&mut pool)
            .ok_or_else(|| "screencopy buffer is still active".to_owned())?
            .to_vec();
        let (_, width, height, stride) = pending
            .offer
            .ok_or_else(|| "screencopy metadata is missing".to_owned())?;
        Ok(ScreencopyFrame {
            width,
            height,
            stride,
            format: pending
                .format
                .ok_or_else(|| "screencopy format is missing".to_owned())?,
            y_invert: pending.y_invert,
            pixels,
            dmabuf: false,
        })
    }

    pub(crate) fn fail_screencopy(&mut self, frame: &ZwlrScreencopyFrameV1, error: String) {
        let request_id = self
            .screencopies
            .iter()
            .position(|pending| pending.frame == *frame)
            .map(|index| self.screencopies.remove(index).request_id);
        frame.destroy();
        if let Some(request_id) = request_id {
            self.events.push_back(DesktopEvent::Screencopy {
                request_id,
                result: Err(error),
            });
        }
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for DesktopState {
    fn event(
        state: &mut Self,
        proxy: &ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        _data: &(),
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => {
                let format = match format {
                    wayland_client::WEnum::Value(format) => format,
                    wayland_client::WEnum::Unknown(value) => {
                        state.fail_screencopy(proxy, format!("unknown screencopy format {value}"));
                        return;
                    }
                };
                if let Some(pending) = state
                    .screencopies
                    .iter_mut()
                    .find(|pending| pending.frame == *proxy)
                {
                    pending.offer = Some((format, width, height, stride));
                }
                if proxy.version() < 3
                    && let Err(error) = state.start_screencopy(proxy)
                {
                    state.fail_screencopy(proxy, error);
                }
            }
            zwlr_screencopy_frame_v1::Event::BufferDone => {
                if let Err(error) = state.start_screencopy(proxy) {
                    state.fail_screencopy(proxy, error);
                }
            }
            zwlr_screencopy_frame_v1::Event::Flags { flags } => {
                if let wayland_client::WEnum::Value(flags) = flags
                    && let Some(pending) = state
                        .screencopies
                        .iter_mut()
                        .find(|pending| pending.frame == *proxy)
                {
                    pending.y_invert = flags.contains(zwlr_screencopy_frame_v1::Flags::YInvert);
                }
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => {
                let Some(request_id) = state
                    .screencopies
                    .iter()
                    .find(|pending| pending.frame == *proxy)
                    .map(|pending| pending.request_id)
                else {
                    return;
                };
                let result = state.finish_screencopy(proxy);
                proxy.destroy();
                state
                    .events
                    .push_back(DesktopEvent::Screencopy { request_id, result });
            }
            zwlr_screencopy_frame_v1::Event::Failed => {
                state.fail_screencopy(proxy, "compositor rejected screencopy".to_owned());
            }
            _ => {}
        }
    }
}
