//! A ring's buffers shown on a window: dmabufs this engine exports, handed
//! to the window's [`BufferSink`] and fenced here.
//!
//! This is what Mesa's Vulkan WSI does for a swapchain, done by the engine
//! so that it knows what is in each buffer (see `present`). The window
//! system's half -- wrapping a dmabuf as a buffer, attaching, committing,
//! hearing releases -- is the sink's, which the window's backend
//! (`morf-app`) supplies; this side never names a window system.
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

use std::os::fd::{AsFd, AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use ash::vk;
use morf_value::present::{BufferSink, Damage, DmabufPlane, SinkBuffer};
use wgpu::hal::api::Vulkan;

use super::dmabuf::{self, DmabufImage, FOURCC_ARGB8888, Purpose};
use super::present::Slot;
use crate::DamageRect;

/// A slot's dmabuf, the window system's buffer for it, and its two
/// semaphores.
pub(crate) struct SlotLink {
    buffer: Box<dyn SinkBuffer>,
    image: DmabufImage,
    /// Signalled by the submission that finishes a frame in it.
    signal: vk::Semaphore,
    /// Imported from the compositor's fences before it is drawn into again.
    wait: vk::Semaphore,
    raw: ash::Device,
}

impl SlotLink {
    /// Whether nothing reads or writes the buffer any more: every fence its
    /// dmabuf carries has signalled.
    pub(crate) fn idle(&self) -> bool {
        export_sync_file(self.image.plane.fd.as_raw_fd(), DMA_BUF_SYNC_WRITE)
            .is_none_or(|fence| signalled(&fence))
    }
}

impl Drop for SlotLink {
    fn drop(&mut self) {
        // A slot is dropped released, and a release comes after the
        // compositor's read, which waited on the frame: nothing submitted
        // still refers to the semaphores.
        unsafe {
            self.raw.destroy_semaphore(self.signal, None);
            self.raw.destroy_semaphore(self.wait, None);
        }
    }
}

/// The window a ring presents to, through the sink its backend gave.
pub(crate) struct BufferLink {
    sink: Box<dyn BufferSink>,
    /// Modifiers both the window system and the device take.
    modifiers: Vec<u64>,
    /// Buffers of an old size, kept until the compositor lets go of them.
    retired: Vec<Slot>,
    device: wgpu::Device,
    raw: ash::Device,
    semaphore_fd: ash::khr::external_semaphore_fd::Device,
}

impl BufferLink {
    /// Presents through `sink`, or says why presenting through the engine's
    /// own buffers is not possible here.
    pub(crate) fn connect(
        device: &wgpu::Device,
        support: Option<&dmabuf::DmabufSupport>,
        sink: Box<dyn BufferSink>,
    ) -> Result<Self, String> {
        if !support.is_some_and(|support| support.sync_file) {
            return Err("the device cannot export dmabufs with sync files".to_owned());
        }
        let theirs = sink.modifiers(FOURCC_ARGB8888);
        let ours = dmabuf::modifiers_for_purpose(device, FOURCC_ARGB8888, Purpose::PRESENT);
        let modifiers: Vec<u64> = ours
            .into_iter()
            .filter(|modifier| theirs.contains(modifier))
            .collect();
        if modifiers.is_empty() {
            return Err("the compositor and the GPU agree on no ARGB8888 modifier".to_owned());
        }
        let hal = unsafe { device.as_hal::<Vulkan>() }.ok_or("the device is not Vulkan")?;
        let raw = hal.raw_device().clone();
        let semaphore_fd = ash::khr::external_semaphore_fd::Device::new(
            hal.shared_instance().raw_instance(),
            &raw,
        );
        drop(hal);
        Ok(Self {
            sink,
            modifiers,
            retired: Vec::new(),
            device: device.clone(),
            raw,
            semaphore_fd,
        })
    }

    /// Hears whatever the window system sent the sink: releases, mostly.
    /// The host's loop reads the socket; this only takes what it read.
    pub(crate) fn dispatch(&mut self) {
        self.sink.dispatch();
        self.retired
            .retain(|slot| slot.busy.load(Ordering::Acquire));
    }

    /// Waits for the window system until something arrives or `deadline`
    /// passes -- at least once, without waiting, when it already has.
    /// Returns whether anything did.
    pub(crate) fn wait_release(&mut self, deadline: Instant) -> bool {
        let heard = self.sink.wait(deadline);
        if heard {
            self.dispatch();
        }
        heard
    }

    /// A new buffer of `size`, exported, made the window system's.
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
        let busy = Arc::new(AtomicBool::new(false));
        let plane = DmabufPlane {
            fd: image.plane.fd.as_fd(),
            offset: image.plane.offset,
            stride: image.plane.stride,
            modifier: image.modifier,
        };
        let buffer = match self.sink.import(plane, size, FOURCC_ARGB8888, busy.clone()) {
            Ok(buffer) => buffer,
            Err(error) => {
                unsafe {
                    self.raw.destroy_semaphore(signal, None);
                    self.raw.destroy_semaphore(wait, None);
                }
                return Err(error);
            }
        };
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
        if super::present::log_wanted() {
            eprintln!(
                "{} morf: present: committed {}",
                super::present::stamp(),
                link.buffer.label()
            );
        }
        let damage: Vec<Damage> = damage
            .iter()
            .map(|rect| Damage {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            })
            .collect();
        self.sink.present(link.buffer.as_ref(), &damage);
    }

    /// Commits with the buffer already there: a frame drawn into no buffer
    /// still delivers the frame callback the host asked for.
    pub(crate) fn commit_without_buffer(&mut self) {
        self.sink.commit();
    }
}

impl Drop for BufferLink {
    fn drop(&mut self) {
        // Every semaphore a slot holds may be in a submission still running.
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        self.retired.clear();
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
