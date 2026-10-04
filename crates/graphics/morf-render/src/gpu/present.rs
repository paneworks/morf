//! Presenting into buffers whose contents are known.
//!
//! A swapchain hands back an image whose contents are, as far as wgpu is
//! concerned, undefined: wgpu marks every acquired image uninitialised and
//! clears it before the first pass that loads it. So the frame that moved a
//! clock hand had to write every pixel of the image anyway -- a full read of
//! the persistent target and a full write of the image, per output, per
//! frame. At 4K on an integrated GPU the compositor is already keeping busy,
//! that copy was most of what a frame cost, whatever the damage.
//!
//! Here the buffers are this engine's own. Each one remembers which frame it
//! last showed, and the damage of every frame since is kept, so a buffer
//! coming back into use is brought up to date by copying only what changed
//! while it was away -- the "buffer age" every compositor-side renderer
//! uses. A buffer too old for the history, or new, is copied whole.
//!
//! How the buffers reach the screen is `present_link`'s business: dmabufs
//! handed to the window's `BufferSink`. Without it (the tests) a ring
//! is a set of plain textures whose release the caller decides.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::DamageRect;

mod history;

use history::{DamageHistory, repaint};

use super::targets::{
    CompositePipeline, clamp_scissor, create_composite_bind_group, create_composite_pipeline,
    plain_view,
};

/// The buffer format: `B8G8R8A8`, which is `ARGB8888` to a compositor.
pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// One buffer of the ring.
pub(crate) struct Slot {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// The frame whose picture it holds; `None` until it is first drawn.
    painted: Option<u64>,
    /// Whether the compositor still has it. Cleared by its `release`.
    pub(crate) busy: Arc<AtomicBool>,
    /// The dmabuf and the window system's buffer behind it, when it has any.
    pub(crate) link: Option<super::present_link::SlotLink>,
}

impl Slot {
    pub(crate) fn new(texture: wgpu::Texture, link: Option<super::present_link::SlotLink>) -> Self {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            painted: None,
            busy: Arc::new(AtomicBool::new(false)),
            link,
        }
    }
}

/// Buffers presented in rotation, each brought up to date by its damage.
pub(crate) struct BufferRing {
    pub(crate) slots: Vec<Slot>,
    history: DamageHistory,
    size: (u32, u32),
    /// The most buffers the ring grows to before it waits for one back.
    limit: usize,
    composite: CompositePipeline,
    bind_group: wgpu::BindGroup,
    /// Where the buffers go; `None` in the tests.
    pub(crate) link: Option<super::present_link::BufferLink>,
    /// The slot the frame being recorded is drawn into.
    pub(crate) pending: Option<usize>,
    /// The slot last presented, for the tests to read.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) presented: Option<usize>,
    /// Damage of frames that went to no buffer, still to be declared.
    pub(crate) undeclared: Vec<DamageRect>,
}

impl BufferRing {
    pub(crate) fn new(
        device: &wgpu::Device,
        target: &wgpu::Texture,
        limit: usize,
        link: Option<super::present_link::BufferLink>,
    ) -> Self {
        let composite = create_composite_pipeline(device, FORMAT);
        let bind_group = create_composite_bind_group(
            device,
            &composite.layout,
            &plain_view(target),
            &composite.sampler,
        );
        Self {
            slots: Vec::new(),
            history: DamageHistory::default(),
            size: (target.width(), target.height()),
            limit: limit.max(1),
            composite,
            bind_group,
            link,
            pending: None,
            presented: None,
            undeclared: Vec::new(),
        }
    }

    /// The persistent target was replaced: read the new one, and repaint
    /// every buffer whole. A buffer of another size is dropped.
    pub(crate) fn retarget(&mut self, device: &wgpu::Device, target: &wgpu::Texture) {
        self.bind_group = create_composite_bind_group(
            device,
            &self.composite.layout,
            &plain_view(target),
            &self.composite.sampler,
        );
        let size = (target.width(), target.height());
        if size != self.size {
            self.size = size;
            let old = std::mem::take(&mut self.slots);
            if let Some(link) = &mut self.link {
                link.retire(old);
            }
            self.presented = None;
        }
        for slot in &mut self.slots {
            slot.painted = None;
        }
    }

    /// A free buffer to draw the next frame into, made if there is room:
    /// the freshest free one, since it has the least to catch up on.
    fn acquire(&mut self, device: &wgpu::Device) -> Option<usize> {
        if let Some(link) = &mut self.link {
            link.dispatch();
        }
        let started = std::time::Instant::now();
        let deadline = started + release_wait();
        let acquired = self.acquire_until(device, deadline);
        if log_wanted() {
            let busy = self
                .slots
                .iter()
                .filter(|slot| slot.busy.load(Ordering::Acquire))
                .count();
            eprintln!(
                "{} morf: present: buffer {acquired:?} of {} ({busy} held) after {:.1} ms",
                stamp(),
                self.slots.len(),
                started.elapsed().as_secs_f64() * 1e3
            );
        }
        acquired
    }

    fn acquire_until(
        &mut self,
        device: &wgpu::Device,
        deadline: std::time::Instant,
    ) -> Option<usize> {
        for _ in 0..64 {
            // Given back is not always done with: a compositor may release a
            // buffer while its GPU still reads it. One it has finished with
            // is taken first; drawing into another waits on the GPU for the
            // read, and the queue is every output's, so each of them would
            // wait behind it.
            let freshest = |ready: bool| {
                self.slots
                    .iter()
                    .enumerate()
                    .filter(|(_, slot)| !slot.busy.load(Ordering::Acquire))
                    .filter(|(_, slot)| !ready || slot.link.as_ref().is_none_or(|link| link.idle()))
                    .max_by_key(|(_, slot)| slot.painted.map_or(0, |frame| frame + 1))
                    .map(|(index, _)| index)
            };
            let free = freshest(true).or_else(|| {
                // Room for another is better than waiting on a read.
                (self.slots.len() >= self.limit)
                    .then(|| freshest(false))
                    .flatten()
            });
            if free.is_some() {
                return free;
            }
            if self.slots.len() < self.limit {
                let slot = match &mut self.link {
                    Some(link) => match link.allocate(device, self.size) {
                        Ok(slot) => slot,
                        Err(error) => {
                            eprintln!("morf: gpu: could not make a buffer: {error}");
                            return None;
                        }
                    },
                    None => Slot::new(plain_texture(device, self.size), None),
                };
                self.slots.push(slot);
                return Some(self.slots.len() - 1);
            }
            // All of them are on screen or queued there: the compositor is
            // behind. Wait for one back, as a swapchain's acquire does, but
            // briefly: past `release_wait` the frame is skipped. Its damage
            // is in the history and reaches the next buffer drawn, and the
            // loop owes the paint.
            //
            // Not skipping at once: that drew twice the frames offscreen and
            // handed over half as many. Not a quarter of a second either: a
            // compositor on a busy GPU held all four that long several times
            // a minute, and each time the output froze, input and all.
            let waited = self
                .link
                .as_mut()
                .is_some_and(|link| link.wait_release(deadline));
            if !waited {
                return None;
            }
        }
        None
    }

    /// Records the frame's damage, and the copy of what the next buffer is
    /// missing into it. Returns whether a buffer was drawn into; if not, the
    /// frame is still in the history and the next buffer gets it.
    pub(crate) fn encode(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        damage: &[DamageRect],
    ) -> bool {
        self.history.record(damage);
        self.pending = None;
        self.presented = None;
        let Some(index) = self.acquire(device) else {
            self.undeclared.extend_from_slice(damage);
            return false;
        };
        let slot = &mut self.slots[index];
        let rects = repaint(&self.history, slot.painted, self.size);
        if log_wanted() {
            let area: u64 =
                rects
                    .as_ref()
                    .map_or(u64::from(self.size.0) * u64::from(self.size.1), |rects| {
                        rects
                            .iter()
                            .map(|rect| u64::from(rect.width) * u64::from(rect.height))
                            .sum()
                    });
            eprintln!(
                "{} morf: present: copying {area} px into buffer {index} ({})",
                stamp(),
                slot.painted.map_or("new".to_owned(), |frame| format!(
                    "{} frames old",
                    self.history.frame() - frame
                ))
            );
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("morf buffer composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &slot.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Kept: the buffer holds the frame it last showed, and
                    // only what changed since is copied over it.
                    load: if rects.is_some() {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.composite.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        match &rects {
            None => pass.draw(0..3, 0..1),
            Some(rects) => {
                for rect in rects {
                    if let Some((x, y, width, height)) =
                        clamp_scissor(*rect, self.size.0, self.size.1)
                    {
                        pass.set_scissor_rect(x, y, width, height);
                        pass.draw(0..3, 0..1);
                    }
                }
            }
        }
        drop(pass);
        slot.painted = Some(self.history.frame());
        self.pending = Some(index);
        true
    }

    /// Hands the buffer just drawn to whoever shows it. Called after the
    /// frame's submission; `damage` is the frame's own.
    pub(crate) fn present(&mut self, queue: &wgpu::Queue, damage: &[DamageRect]) {
        let Some(index) = self.pending.take() else {
            // No buffer this frame. The surface is still committed, so the
            // frame callback the host asked for comes, and the host keeps
            // drawing; the damage waits for the next buffer.
            if let Some(link) = &mut self.link {
                link.commit_without_buffer();
            }
            return;
        };
        let slot = &mut self.slots[index];
        slot.busy.store(true, Ordering::Release);
        self.presented = Some(index);
        let mut declared = std::mem::take(&mut self.undeclared);
        declared.extend_from_slice(damage);
        if let Some(link) = &mut self.link {
            link.present(queue, slot, &declared);
        }
    }

    /// Signals for the submission that finishes the pending buffer, when it
    /// needs any. Called just before it is submitted.
    pub(crate) fn before_submit(&mut self, queue: &wgpu::Queue) {
        if let (Some(index), Some(link)) = (self.pending, &mut self.link) {
            link.before_submit(queue, &self.slots[index]);
        }
    }

    #[cfg(test)]
    pub(crate) fn release(&mut self, index: usize) {
        if let Some(slot) = self.slots.get(index) {
            slot.busy.store(false, Ordering::Release);
        }
    }
}

#[cfg(test)]
impl super::backend_types::WgpuBackend {
    /// Presents into a ring of `limit` plain textures from now on, whose
    /// release the test decides.
    pub(crate) fn present_into_ring(&mut self, limit: usize) {
        self.buffers = Some(BufferRing::new(&self.device, &self.texture, limit, None));
    }

    /// The buffer the last frame went to, if it went to one.
    pub(crate) fn ring_presented(&self) -> Option<usize> {
        self.buffers.as_ref()?.presented
    }

    /// The compositor lets go of buffer `index`.
    pub(crate) fn ring_release(&mut self, index: usize) {
        if let Some(ring) = &mut self.buffers {
            ring.release(index);
        }
    }

    /// Buffer `index`'s pixels, as RGBA like `read_pixels`.
    pub(crate) fn ring_pixels(&self, index: usize) -> Vec<u8> {
        let ring = self.buffers.as_ref().expect("a ring");
        let mut pixels = self.read_texture(&ring.slots[index].texture);
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        pixels
    }
}

/// `MORF_PRESENT_LOG=1`: every frame's buffer, how many the compositor
/// held, and how long getting one took.
pub(crate) fn log_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| std::env::var_os("MORF_PRESENT_LOG").is_some())
}

/// `[12345.678]`: monotonic milliseconds, as every diagnostic line starts.
pub(crate) fn stamp() -> String {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `now` is a valid, writable timespec for the call's duration.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    format!(
        "[{:.3}]",
        now.tv_sec as f64 * 1000.0 + now.tv_nsec as f64 / 1_000_000.0
    )
}

/// How long a frame waits for the compositor to give a buffer back before it
/// is skipped: about a refresh and a half, unless `MORF_PRESENT_WAIT_MS`
/// says. The wait holds the output's whole loop -- input, IPC, every other
/// surface -- so it is short: a skipped frame is owed, and painted on the
/// next callback (`WgpuBackend::take_skipped`), rather than waited for.
fn release_wait() -> std::time::Duration {
    static WAIT: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    std::time::Duration::from_millis(*WAIT.get_or_init(|| {
        std::env::var("MORF_PRESENT_WAIT_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(24)
    }))
}

/// A buffer with nothing behind it but a texture: the tests' ring.
fn plain_texture(device: &wgpu::Device, (width, height): (u32, u32)) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("morf ring buffer"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Every submission of the process goes through here.
///
/// The devices are shared between outputs, and so is the queue. Presenting
/// into a dmabuf adds a semaphore to the *next* submission on the queue;
/// another output's thread submitting in between would take it, and signal
/// the compositor before this frame was drawn. So one submission at a time,
/// with whatever was staged for it.
pub(crate) fn submit(
    queue: &wgpu::Queue,
    buffers: impl IntoIterator<Item = wgpu::CommandBuffer>,
    stage: impl FnOnce(),
) -> wgpu::SubmissionIndex {
    let _held = submissions();
    stage();
    queue.submit(buffers)
}

/// Holds every submission of the process off, for one made past wgpu on the
/// raw queue: Vulkan wants a queue's submissions one at a time.
pub(crate) fn submissions() -> std::sync::MutexGuard<'static, ()> {
    static SUBMIT: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SUBMIT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
