//! A ring's buffers on a Wayland surface: dmabufs this engine exports,
//! handed to the compositor as `wl_buffer`s, attached and committed here.
//!
//! This is what Mesa's Vulkan WSI does for a swapchain, done by the engine
//! so that it knows what is in each buffer (see `present`). It talks to the
//! compositor through the surface's own connection, on an event queue of its
//! own -- the way a driver does -- so the host hands over a window handle
//! exactly as it would for a swapchain and never sees any of it.
//!
//! Synchronisation is by sync file, both ways, the same as Mesa's on a
//! kernel that can:
//!
//! - a frame's submission signals a semaphore, exported as a sync file and
//!   attached to the dmabuf as a write fence, so the compositor's read waits
//!   for the frame to be finished rather than this engine waiting for it;
//! - a buffer the compositor released may still be being read by its GPU;
//!   the dmabuf's fences are exported and waited on by the submission that
//!   draws into it again.
//!
//! The images use single-plane modifiers only (`dmabuf::exportable_modifiers`),
//! which carry no compression metadata: their memory is the picture in any
//! layout, so the queue-family transfers a multi-plane image would need do
//! nothing here, and are left out -- they would need submissions of their own
//! outside wgpu's ordering.

use std::any::Any;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use ash::vk;
use wayland_client::backend::{Backend, ObjectId};
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
use wgpu::hal::api::Vulkan;

use super::dmabuf::{self, DmabufImage, FOURCC_ARGB8888, Purpose};
use super::present::Slot;
use crate::DamageRect;

/// What the compositor said on this engine's queue.
#[derive(Default)]
pub(crate) struct LinkState {
    /// The modifiers it takes `ARGB8888` with.
    modifiers: Vec<u64>,
}

impl Dispatch<WlRegistry, GlobalListContents> for LinkState {
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

impl Dispatch<ZwpLinuxDmabufV1, ()> for LinkState {
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
            && format == FOURCC_ARGB8888
        {
            state
                .modifiers
                .push((u64::from(modifier_hi) << 32) | u64::from(modifier_lo));
        }
    }
}

impl Dispatch<ZwpLinuxBufferParamsV1, ()> for LinkState {
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

impl Dispatch<WlBuffer, Arc<AtomicBool>> for LinkState {
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

/// A slot's dmabuf, its `wl_buffer`, and its two semaphores.
pub(crate) struct SlotLink {
    buffer: WlBuffer,
    image: DmabufImage,
    /// Signalled by the submission that finishes a frame in it.
    signal: vk::Semaphore,
    /// Imported from the compositor's fences before it is drawn into again.
    wait: vk::Semaphore,
    raw: ash::Device,
}

impl Drop for SlotLink {
    fn drop(&mut self) {
        self.buffer.destroy();
        // A slot is dropped released, and a release comes after the
        // compositor's read, which waited on the frame: nothing submitted
        // still refers to the semaphores.
        unsafe {
            self.raw.destroy_semaphore(self.signal, None);
            self.raw.destroy_semaphore(self.wait, None);
        }
    }
}

/// The surface a ring presents to, on the surface's own connection.
pub(crate) struct WaylandLink {
    connection: Connection,
    queue: EventQueue<LinkState>,
    state: LinkState,
    surface: WlSurface,
    dmabuf: ZwpLinuxDmabufV1,
    /// Modifiers both the compositor and the device take.
    modifiers: Vec<u64>,
    /// Buffers of an old size, kept until the compositor lets go of them.
    retired: Vec<Slot>,
    device: wgpu::Device,
    raw: ash::Device,
    semaphore_fd: ash::khr::external_semaphore_fd::Device,
    /// Whatever the host handed over for the window: it keeps the surface's
    /// connection alive as long as this is.
    _window: Box<dyn Any + Send + Sync>,
}

impl WaylandLink {
    /// Connects to the surface `surface` of the display `display`, or says
    /// why presenting through the engine's own buffers is not possible here
    /// and hands `window` back.
    ///
    /// # Safety
    ///
    /// `display` and `surface` must be a live `wl_display` and a `wl_surface`
    /// on it, kept alive by `window`.
    pub(crate) unsafe fn connect(
        device: &wgpu::Device,
        support: Option<&dmabuf::DmabufSupport>,
        handles: (NonNull<std::ffi::c_void>, NonNull<std::ffi::c_void>),
        window: Box<dyn Any + Send + Sync>,
    ) -> Result<Self, (String, Box<dyn Any + Send + Sync>)> {
        match unsafe { Self::open(device, support, handles) } {
            Ok(mut link) => {
                link._window = window;
                Ok(link)
            }
            Err(error) => Err((error, window)),
        }
    }

    unsafe fn open(
        device: &wgpu::Device,
        support: Option<&dmabuf::DmabufSupport>,
        (display, surface): (NonNull<std::ffi::c_void>, NonNull<std::ffi::c_void>),
    ) -> Result<Self, String> {
        if !support.is_some_and(|support| support.sync_file) {
            return Err("the device cannot export dmabufs with sync files".to_owned());
        }
        let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, mut queue) = registry_queue_init::<LinkState>(&connection)
            .map_err(|error| format!("could not list the globals: {error}"))?;
        let qh = queue.handle();
        // Version 3: the one that lists formats and modifiers as events.
        let dmabuf: ZwpLinuxDmabufV1 = globals
            .bind(&qh, 3..=3, ())
            .map_err(|error| format!("no zwp_linux_dmabuf_v1 v3: {error}"))?;
        let mut state = LinkState::default();
        queue
            .roundtrip(&mut state)
            .map_err(|error| format!("could not hear the dmabuf formats: {error}"))?;
        let ours = dmabuf::modifiers_for_purpose(device, FOURCC_ARGB8888, Purpose::PRESENT);
        let modifiers: Vec<u64> = ours
            .into_iter()
            .filter(|modifier| state.modifiers.contains(modifier))
            .collect();
        if modifiers.is_empty() {
            dmabuf.destroy();
            return Err("the compositor and the GPU agree on no ARGB8888 modifier".to_owned());
        }
        let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.as_ptr().cast()) }
            .map_err(|_| "the window is not a wl_surface".to_owned())?;
        let surface =
            WlSurface::from_id(&connection, id).map_err(|_| "the wl_surface is gone".to_owned())?;
        let hal = unsafe { device.as_hal::<Vulkan>() }.ok_or("the device is not Vulkan")?;
        let raw = hal.raw_device().clone();
        let semaphore_fd = ash::khr::external_semaphore_fd::Device::new(
            hal.shared_instance().raw_instance(),
            &raw,
        );
        drop(hal);
        Ok(Self {
            connection,
            queue,
            state,
            surface,
            dmabuf,
            modifiers,
            retired: Vec::new(),
            device: device.clone(),
            raw,
            semaphore_fd,
            _window: Box::new(()),
        })
    }

    /// Hears whatever the compositor sent this queue: releases, mostly. The
    /// host's loop reads the socket; this only takes what it read.
    pub(crate) fn dispatch(&mut self) {
        let _ = self.queue.dispatch_pending(&mut self.state);
        self.retired
            .retain(|slot| slot.busy.load(Ordering::Acquire));
    }

    /// Reads the socket for this queue until something arrives or `deadline`
    /// passes. Returns whether anything did.
    pub(crate) fn wait_release(&mut self, deadline: Instant) -> bool {
        loop {
            if self.queue.dispatch_pending(&mut self.state).unwrap_or(0) > 0 {
                self.dispatch();
                return true;
            }
            let _ = self.connection.flush();
            let Some(guard) = self.queue.prepare_read() else {
                continue;
            };
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            let mut poll = [libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            }];
            let ready = unsafe {
                libc::poll(
                    poll.as_mut_ptr(),
                    1,
                    left.as_millis().clamp(1, i32::MAX as u128) as i32,
                )
            };
            if ready > 0 {
                if guard.read().is_err() {
                    return false;
                }
            } else {
                drop(guard);
                if ready == 0 {
                    return false;
                }
            }
        }
    }

    /// A new buffer of `size`, exported, wrapped as a `wl_buffer`.
    pub(crate) fn allocate(
        &mut self,
        device: &wgpu::Device,
        size: (u32, u32),
    ) -> Result<Slot, String> {
        let image = dmabuf::export_for(
            device,
            size,
            FOURCC_ARGB8888,
            &self.modifiers,
            Purpose::PRESENT,
        )?;
        let exportable = |raw: &ash::Device| {
            let mut export = vk::ExportSemaphoreCreateInfo::default()
                .handle_types(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
            unsafe {
                raw.create_semaphore(
                    &vk::SemaphoreCreateInfo::default().push_next(&mut export),
                    None,
                )
            }
            .map_err(|error| format!("could not create a semaphore: {error}"))
        };
        let signal = exportable(&self.raw)?;
        let wait = match unsafe {
            self.raw
                .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
        } {
            Ok(wait) => wait,
            Err(error) => {
                unsafe { self.raw.destroy_semaphore(signal, None) };
                return Err(format!("could not create a semaphore: {error}"));
            }
        };
        let qh = self.queue.handle();
        let params = self.dmabuf.create_params(&qh, ());
        params.add(
            image.plane.fd.as_fd(),
            0,
            image.plane.offset,
            image.plane.stride,
            (image.modifier >> 32) as u32,
            (image.modifier & 0xffff_ffff) as u32,
        );
        let busy = Arc::new(AtomicBool::new(false));
        let buffer = params.create_immed(
            size.0 as i32,
            size.1 as i32,
            FOURCC_ARGB8888,
            zwp_linux_buffer_params_v1::Flags::empty(),
            &qh,
            busy.clone(),
        );
        params.destroy();
        let texture = image.texture.clone();
        let mut slot = Slot::new(
            texture,
            Some(SlotLink {
                buffer,
                image,
                signal,
                wait,
                raw: self.raw.clone(),
            }),
        );
        slot.busy = busy;
        Ok(slot)
    }

    /// Keeps the buffers of an old size until the compositor lets go of them.
    pub(crate) fn retire(&mut self, slots: Vec<Slot>) {
        self.retired.extend(
            slots
                .into_iter()
                .filter(|slot| slot.busy.load(Ordering::Acquire)),
        );
    }

    /// Stages the semaphores of the submission drawing into `slot`: a wait
    /// for whatever of the compositor's still reads it, and the signal that
    /// says the frame is done. Called with the queue's submissions held.
    pub(crate) fn before_submit(&mut self, queue: &wgpu::Queue, slot: &Slot) {
        let Some(link) = &slot.link else {
            return;
        };
        let Some(hal) = (unsafe { queue.as_hal::<Vulkan>() }) else {
            return;
        };
        if let Some(fence) = export_sync_file(link.image.plane.fd.as_raw_fd(), DMA_BUF_SYNC_WRITE)
            && !signalled(&fence)
        {
            let fd = fence.into_raw_fd();
            let imported = unsafe {
                self.semaphore_fd.import_semaphore_fd(
                    &vk::ImportSemaphoreFdInfoKHR::default()
                        .semaphore(link.wait)
                        .flags(vk::SemaphoreImportFlags::TEMPORARY)
                        .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
                        .fd(fd),
                )
            };
            match imported {
                // The fd is the semaphore's now.
                Ok(()) => hal.add_wait_semaphore(
                    link.wait,
                    None,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                ),
                Err(_) => {
                    // Waited for here instead; it is almost always done.
                    let fence = unsafe { OwnedFd::from_raw_fd(fd) };
                    wait_signalled(&fence, 1000);
                }
            }
        }
        hal.add_signal_semaphore(link.signal, None);
    }

    /// Attaches `slot`'s buffer, declares `damage`, and commits: the frame
    /// goes out with a fence the compositor waits on.
    pub(crate) fn present(&mut self, _queue: &wgpu::Queue, slot: &Slot, damage: &[DamageRect]) {
        let Some(link) = &slot.link else {
            return;
        };
        let exported = unsafe {
            self.semaphore_fd.get_semaphore_fd(
                &vk::SemaphoreGetFdInfoKHR::default()
                    .semaphore(link.signal)
                    .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD),
            )
        };
        match exported {
            // -1: already signalled, nothing to wait for.
            Ok(fd) if fd >= 0 => {
                let fence = unsafe { OwnedFd::from_raw_fd(fd) };
                if !import_sync_file(link.image.plane.fd.as_raw_fd(), &fence) {
                    // A kernel without the import: the compositor would read
                    // a frame half drawn, so it is waited for here instead.
                    wait_signalled(&fence, 1000);
                }
            }
            Ok(_) => {}
            Err(_) => {
                let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            }
        }
        self.surface.attach(Some(&link.buffer), 0, 0);
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

    /// Commits with the buffer already there: a frame drawn into no buffer
    /// still delivers the frame callback the host asked for.
    pub(crate) fn commit_without_buffer(&mut self) {
        self.surface.commit();
        let _ = self.connection.flush();
    }
}

impl Drop for WaylandLink {
    fn drop(&mut self) {
        // Every semaphore a slot holds may be in a submission still running.
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        self.retired.clear();
        self.dmabuf.destroy();
        let _ = self.connection.flush();
    }
}

const DMA_BUF_SYNC_WRITE: u32 = 2;
/// `_IOWR('b', 2, struct dma_buf_export_sync_file)`.
const DMA_BUF_IOCTL_EXPORT_SYNC_FILE: u64 = 0xC008_6202;
/// `_IOW('b', 3, struct dma_buf_import_sync_file)`.
const DMA_BUF_IOCTL_IMPORT_SYNC_FILE: u64 = 0x4008_6203;

#[repr(C)]
struct SyncFileArgument {
    flags: u32,
    fd: i32,
}

/// The fences a dmabuf carries, as one sync file: with `DMA_BUF_SYNC_WRITE`,
/// every one a writer has to wait for.
fn export_sync_file(dmabuf: RawFd, flags: u32) -> Option<OwnedFd> {
    let mut argument = SyncFileArgument { flags, fd: -1 };
    let status = unsafe {
        libc::ioctl(
            dmabuf,
            DMA_BUF_IOCTL_EXPORT_SYNC_FILE as _,
            &mut argument as *mut SyncFileArgument,
        )
    };
    (status == 0 && argument.fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(argument.fd) })
}

/// Attaches a sync file to a dmabuf as a write fence. Returns whether the
/// kernel took it.
fn import_sync_file(dmabuf: RawFd, fence: &OwnedFd) -> bool {
    let mut argument = SyncFileArgument {
        flags: DMA_BUF_SYNC_WRITE,
        fd: fence.as_raw_fd(),
    };
    let status = unsafe {
        libc::ioctl(
            dmabuf,
            DMA_BUF_IOCTL_IMPORT_SYNC_FILE as _,
            &mut argument as *mut SyncFileArgument,
        )
    };
    status == 0
}

fn signalled(fence: &OwnedFd) -> bool {
    poll_fence(fence, 0)
}

fn wait_signalled(fence: &OwnedFd, milliseconds: i32) {
    poll_fence(fence, milliseconds);
}

/// A sync file polls readable once its fence has signalled.
fn poll_fence(fence: &OwnedFd, milliseconds: i32) -> bool {
    let mut poll = [libc::pollfd {
        fd: fence.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    }];
    unsafe { libc::poll(poll.as_mut_ptr(), 1, milliseconds) > 0 }
}
