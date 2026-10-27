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

mod sync_file;

use std::os::fd::{AsFd, AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use ash::vk;
use morf_value::present::{BufferSink, Damage, DmabufPlane, SinkBuffer};
use wgpu::hal::api::Vulkan;

use super::depth::PresentFormat;
use super::dmabuf::{self, DmabufImage, Purpose};
use super::present::Slot;
use crate::DamageRect;
use sync_file::{
    DMA_BUF_SYNC_WRITE, export_sync_file, import_sync_file, signalled, wait_signalled,
};

/// The modifiers both the window and the device take `format` buffers with.
///
/// Linear whenever both sides can: a presented buffer is never drawn into,
/// only copied to (the frame is drawn in a target of its own), so tiling it
/// buys nothing, and linear is the one layout every importer reads the same
/// way. A tiled one, left to the driver to pick, came out garbled under
/// Hyprland on a Kaby Lake laptop -- blocks of noise, text in pieces --
/// where a swapchain was fine.
fn agreed(device: &wgpu::Device, sink: &dyn BufferSink, format: PresentFormat) -> Vec<u64> {
    let theirs = sink.modifiers(format.fourcc());
    let ours = dmabuf::modifiers_for_purpose(device, format.fourcc(), Purpose::PRESENT);
    let shared: Vec<u64> = ours
        .iter()
        .copied()
        .filter(|modifier| theirs.contains(modifier))
        .filter(|modifier| dmabuf::modifier_wanted(*modifier))
        .collect();
    let modifiers = if shared.contains(&dmabuf::MODIFIER_LINEAR) {
        vec![dmabuf::MODIFIER_LINEAR]
    } else {
        shared
    };
    if morf_value::env::log("gpu") {
        eprintln!(
            "morf: gpu: {format:?} present modifiers: ours {}, the compositor's {}, used {}",
            dmabuf::modifier_names(&ours),
            dmabuf::modifier_names(&theirs),
            dmabuf::modifier_names(&modifiers),
        );
    }
    modifiers
}

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
    /// `None` only once [`BufferLink::into_sink`] took it.
    sink: Option<Box<dyn BufferSink>>,
    /// Modifiers both the window system and the device take, by format.
    modifiers: [(PresentFormat, Vec<u64>); 3],
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
        let modifiers = PresentFormat::ALL.map(|format| (format, agreed(device, &*sink, format)));
        if modifiers[0].1.is_empty() {
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
            sink: Some(sink),
            modifiers,
            retired: Vec::new(),
            device: device.clone(),
            raw,
            semaphore_fd,
        })
    }

    /// The window's sink, for a link on another device: the buffers made
    /// here go with this link, the window stays.
    pub(crate) fn into_sink(mut self) -> Option<Box<dyn BufferSink>> {
        self.sink.take()
    }

    /// Hears whatever the window system sent the sink: releases, mostly.
    /// The host's loop reads the socket; this only takes what it read.
    pub(crate) fn dispatch(&mut self) {
        if let Some(sink) = &mut self.sink {
            sink.dispatch();
        }
        self.retired
            .retain(|slot| slot.busy.load(Ordering::Acquire));
    }

    /// Waits for the window system until something arrives or `deadline`
    /// passes -- at least once, without waiting, when it already has.
    /// Returns whether anything did.
    pub(crate) fn wait_release(&mut self, deadline: Instant) -> bool {
        let heard = self.sink.as_mut().is_some_and(|sink| sink.wait(deadline));
        if heard {
            self.dispatch();
        }
        heard
    }

    /// What the window system can be told about the buffers' colours.
    pub(crate) fn color_support(&self) -> morf_value::present::ColorSupport {
        self.sink
            .as_ref()
            .map(|sink| sink.color_support())
            .unwrap_or_default()
    }

    /// Says how the buffers presented from the next commit on are encoded.
    pub(crate) fn set_encoding(
        &mut self,
        encoding: morf_value::present::Encoding,
    ) -> Result<(), String> {
        let sink = self.sink.as_mut().ok_or("the link has no window")?;
        sink.set_encoding(encoding)
    }

    /// Whether the window and the device agree on a modifier for `format`.
    pub(crate) fn takes(&self, format: PresentFormat) -> bool {
        self.modifiers_of(format).is_some()
    }

    fn modifiers_of(&self, format: PresentFormat) -> Option<&[u64]> {
        self.modifiers
            .iter()
            .find(|(each, modifiers)| *each == format && !modifiers.is_empty())
            .map(|(_, modifiers)| modifiers.as_slice())
    }

    /// A new `format` buffer of `size`, exported, made the window system's.
    pub(crate) fn allocate(
        &mut self,
        device: &wgpu::Device,
        size: (u32, u32),
        format: PresentFormat,
    ) -> Result<Slot, String> {
        let modifiers = self
            .modifiers_of(format)
            .map(<[u64]>::to_vec)
            .ok_or_else(|| format!("the compositor takes no {format:?} buffer"))?;
        let image =
            dmabuf::export_for(device, size, format.fourcc(), &modifiers, Purpose::PRESENT)?;
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
        let imported = match &mut self.sink {
            Some(sink) => sink.import(plane, size, format.fourcc(), busy.clone()),
            None => Err("the link has no window".to_owned()),
        };
        let buffer = match imported {
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
                Err(error) => {
                    // Waited for here instead; it is almost always done.
                    fallback_once(&SEMAPHORE_FALLBACK, || {
                        format!("the driver would not import a fence ({error})")
                    });
                    let fence = unsafe { OwnedFd::from_raw_fd(fd) };
                    wait_signalled(&fence, FENCE_WAIT_MS);
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
                    fallback_once(&IMPORT_FALLBACK, || {
                        format!(
                            "the kernel refused DMA_BUF_IOCTL_IMPORT_SYNC_FILE ({})",
                            std::io::Error::last_os_error()
                        )
                    });
                    wait_signalled(&fence, FENCE_WAIT_MS);
                }
            }
            Ok(_) => {}
            Err(_) => {
                super::present_stalls::count(&super::present_stalls::FENCE_WAITS);
                let waited = self.device.poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(std::time::Duration::from_millis(FENCE_WAIT_MS as u64)),
                });
                if waited.is_err() {
                    super::present_stalls::count(&super::present_stalls::FENCE_TIMEOUTS);
                }
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
        if let Some(sink) = &mut self.sink {
            sink.present(link.buffer.as_ref(), &damage);
        }
    }

    /// Commits with the buffer already there: a frame drawn into no buffer
    /// still delivers the frame callback the host asked for.
    pub(crate) fn commit_without_buffer(&mut self) {
        if let Some(sink) = &mut self.sink {
            sink.commit();
        }
    }
}

impl Drop for BufferLink {
    fn drop(&mut self) {
        // Every semaphore a slot holds may be in a submission still running.
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        self.retired.clear();
    }
}

/// The longest a fence is waited for on the CPU before the frame goes out.
const FENCE_WAIT_MS: i32 = 1000;

static SEMAPHORE_FALLBACK: std::sync::Once = std::sync::Once::new();
static IMPORT_FALLBACK: std::sync::Once = std::sync::Once::new();

/// Says once per process that a fence is being waited for on the CPU.
fn fallback_once(said: &std::sync::Once, why: impl FnOnce() -> String) {
    said.call_once(|| {
        eprintln!(
            "morf: gpu: {}; waiting for frames on the CPU instead",
            why()
        )
    });
}
