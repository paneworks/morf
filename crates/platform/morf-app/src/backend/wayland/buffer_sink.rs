//! A window's dmabufs shown on its `wl_surface`: `zwp_linux_dmabuf_v1`
//! buffers, attached and committed, their releases heard.
//!
//! The sink talks to the compositor on an event queue of its own on the
//! window's connection -- the way a driver's swapchain does -- so it never
//! touches the host's queue, and the host's loop reading the socket is
//! enough to wake it.

use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use morf_value::present::{BufferSink, Damage, DmabufPlane, SinkBuffer};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_buffer::{self, WlBuffer},
    wl_registry::WlRegistry,
    wl_surface::WlSurface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_buffer_params_v1::{self, ZwpLinuxBufferParamsV1},
    zwp_linux_dmabuf_v1::{self, ZwpLinuxDmabufV1},
};

use crate::backend::wayland::WaylandWindowTarget;

/// What the compositor said on the sink's queue.
#[derive(Default)]
struct SinkState {
    /// Every (format, modifier) it takes.
    formats: Vec<(u32, u64)>,
}

impl Dispatch<WlRegistry, GlobalListContents> for SinkState {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpLinuxDmabufV1, ()> for SinkState {
    fn event(
        state: &mut Self,
        _: &ZwpLinuxDmabufV1,
        event: zwp_linux_dmabuf_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_linux_dmabuf_v1::Event::Modifier {
            format,
            modifier_hi,
            modifier_lo,
        } = event
        {
            state
                .formats
                .push((format, (u64::from(modifier_hi) << 32) | u64::from(modifier_lo)));
        }
    }
}

impl Dispatch<ZwpLinuxBufferParamsV1, ()> for SinkState {
    fn event(
        _: &mut Self,
        _: &ZwpLinuxBufferParamsV1,
        _: zwp_linux_buffer_params_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Only the deferred `create` answers here; buffers are made with
        // `create_immed`.
    }
}

impl Dispatch<WlBuffer, Arc<AtomicBool>> for SinkState {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        event: wl_buffer::Event,
        busy: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            busy.store(false, Ordering::Release);
        }
    }
}

/// A dmabuf as a `wl_buffer`, destroyed with it.
struct WaylandBuffer(WlBuffer);

impl SinkBuffer for WaylandBuffer {
    fn label(&self) -> u32 {
        self.0.id().protocol_id()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Drop for WaylandBuffer {
    fn drop(&mut self) {
        self.0.destroy();
    }
}

/// A `wl_surface` showing dmabufs.
pub struct WaylandBufferSink {
    connection: Connection,
    queue: EventQueue<SinkState>,
    state: SinkState,
    surface: WlSurface,
    dmabuf: ZwpLinuxDmabufV1,
}

impl WaylandWindowTarget {
    /// A sink for this window's dmabufs, or why the compositor cannot take
    /// them.
    pub fn buffer_sink(&self) -> Result<Box<dyn BufferSink>, String> {
        let connection = Connection::from_backend(self.backend.clone());
        let (globals, mut queue) = registry_queue_init::<SinkState>(&connection)
            .map_err(|error| format!("could not list the globals: {error}"))?;
        let qh = queue.handle();
        // Version 3: the one that lists formats and modifiers as events.
        let dmabuf: ZwpLinuxDmabufV1 = globals
            .bind(&qh, 3..=3, ())
            .map_err(|error| format!("no zwp_linux_dmabuf_v1 v3: {error}"))?;
        let mut state = SinkState::default();
        queue
            .roundtrip(&mut state)
            .map_err(|error| format!("could not hear the dmabuf formats: {error}"))?;
        Ok(Box::new(WaylandBufferSink {
            connection,
            queue,
            state,
            surface: self.surface.clone(),
            dmabuf,
        }))
    }
}

impl BufferSink for WaylandBufferSink {
    fn modifiers(&self, fourcc: u32) -> Vec<u64> {
        self.state
            .formats
            .iter()
            .filter(|(format, _)| *format == fourcc)
            .map(|(_, modifier)| *modifier)
            .collect()
    }

    fn import(
        &mut self,
        plane: DmabufPlane<'_>,
        size: (u32, u32),
        fourcc: u32,
        busy: Arc<AtomicBool>,
    ) -> Result<Box<dyn SinkBuffer>, String> {
        let qh = self.queue.handle();
        let params = self.dmabuf.create_params(&qh, ());
        params.add(
            plane.fd,
            0,
            plane.offset,
            plane.stride,
            (plane.modifier >> 32) as u32,
            (plane.modifier & 0xffff_ffff) as u32,
        );
        let buffer = params.create_immed(
            size.0 as i32,
            size.1 as i32,
            fourcc,
            zwp_linux_buffer_params_v1::Flags::empty(),
            &qh,
            busy,
        );
        params.destroy();
        Ok(Box::new(WaylandBuffer(buffer)))
    }

    fn present(&mut self, buffer: &dyn SinkBuffer, damage: &[Damage]) {
        let Some(WaylandBuffer(buffer)) = buffer.as_any().downcast_ref::<WaylandBuffer>() else {
            return;
        };
        self.surface.attach(Some(buffer), 0, 0);
        for rect in damage {
            self.surface.damage_buffer(
                rect.x as i32,
                rect.y as i32,
                rect.width as i32,
                rect.height as i32,
            );
        }
        self.surface.commit();
        let _ = self.connection.flush();
    }

    fn commit(&mut self) {
        self.surface.commit();
        let _ = self.connection.flush();
    }

    fn dispatch(&mut self) {
        let _ = self.queue.dispatch_pending(&mut self.state);
    }

    fn wait(&mut self, deadline: Instant) -> bool {
        // Bounded: a socket busy with other queues' events cannot hold it.
        for _ in 0..64 {
            if self.queue.dispatch_pending(&mut self.state).unwrap_or(0) > 0 {
                return true;
            }
            let _ = self.connection.flush();
            let Some(guard) = self.queue.prepare_read() else {
                continue;
            };
            let left = deadline.saturating_duration_since(Instant::now());
            let timeout = timespec(left);
            let ready = {
                let fd = guard.connection_fd();
                let mut fds = [PollFd::new(&fd, PollFlags::IN)];
                poll(&mut fds, Some(&timeout))
            };
            match ready {
                Ok(ready) if ready > 0 => {
                    if guard.read().is_err() {
                        return false;
                    }
                }
                Ok(_) => return false,
                Err(_) => drop(guard),
            }
        }
        false
    }
}

impl Drop for WaylandBufferSink {
    fn drop(&mut self) {
        self.dmabuf.destroy();
        let _ = self.connection.flush();
    }
}

fn timespec(left: Duration) -> Timespec {
    Timespec {
        tv_sec: left.as_secs().min(i64::MAX as u64) as i64,
        tv_nsec: i64::from(left.subsec_nanos()),
    }
}
