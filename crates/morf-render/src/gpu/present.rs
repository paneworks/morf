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
//! How the buffers reach the screen is `present_wayland`'s business: dmabufs
//! handed to the compositor as `wl_buffer`s. Without it (the tests) a ring
//! is a set of plain textures whose release the caller decides.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::DamageRect;

use super::targets::{
    CompositePipeline, clamp_scissor, create_composite_bind_group, create_composite_pipeline,
    intersect_damage, plain_view, union_damage,
};

/// How many frames of damage are remembered. A buffer that missed more than
/// this is copied whole; with two or three buffers in rotation it never has.
const HISTORY: usize = 16;

/// Past this many rectangles a repaint is copied as their bounds: each is a
/// draw, and a hundred small ones cost more than one that covers them.
const MAX_RECTS: usize = 16;

/// The buffer format: `B8G8R8A8`, which is `ARGB8888` to a compositor.
pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// The damage of the last frames, by frame number.
#[derive(Debug, Default)]
pub(crate) struct DamageHistory {
    /// The number of the latest frame; the first is 1.
    frame: u64,
    frames: VecDeque<(u64, Vec<DamageRect>)>,
}

impl DamageHistory {
    /// Records a frame's damage and returns its number.
    pub(crate) fn record(&mut self, damage: &[DamageRect]) -> u64 {
        self.frame += 1;
        self.frames.push_back((self.frame, damage.to_vec()));
        while self.frames.len() > HISTORY {
            self.frames.pop_front();
        }
        self.frame
    }

    /// The latest frame's number.
    pub(crate) fn frame(&self) -> u64 {
        self.frame
    }

    /// What changed after frame `painted`, up to the latest: `None` when the
    /// buffer has to be copied whole -- it was never painted, or the history
    /// no longer reaches back to it.
    pub(crate) fn since(&self, painted: Option<u64>) -> Option<Vec<DamageRect>> {
        let painted = painted?;
        if painted >= self.frame {
            return Some(Vec::new());
        }
        let oldest = self.frames.front()?.0;
        if painted + 1 < oldest {
            return None;
        }
        Some(
            self.frames
                .iter()
                .filter(|(frame, _)| *frame > painted)
                .flat_map(|(_, rects)| rects.iter().copied())
                .collect(),
        )
    }
}

/// What to copy into a buffer, in surface pixels: `None` is all of it.
pub(crate) fn repaint(
    history: &DamageHistory,
    painted: Option<u64>,
    size: (u32, u32),
) -> Option<Vec<DamageRect>> {
    let whole = DamageRect {
        x: 0,
        y: 0,
        width: size.0,
        height: size.1,
    };
    let mut rects: Vec<DamageRect> = history
        .since(painted)?
        .into_iter()
        .filter_map(|rect| intersect_damage(rect, whole))
        .collect();
    // Rectangles one inside another are one rectangle.
    rects.sort_by_key(|rect| std::cmp::Reverse(u64::from(rect.width) * u64::from(rect.height)));
    let mut kept: Vec<DamageRect> = Vec::with_capacity(rects.len());
    for rect in rects {
        if !kept.iter().any(|seen| contains(*seen, rect)) {
            kept.push(rect);
        }
    }
    if kept.len() > MAX_RECTS {
        let bounds = kept
            .iter()
            .copied()
            .reduce(union_damage)
            .expect("more than none");
        kept = vec![bounds];
    }
    Some(kept)
}

fn contains(outer: DamageRect, inner: DamageRect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

/// One buffer of the ring.
pub(crate) struct Slot {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// The frame whose picture it holds; `None` until it is first drawn.
    painted: Option<u64>,
    /// Whether the compositor still has it. Cleared by its `release`.
    pub(crate) busy: Arc<AtomicBool>,
    /// The dmabuf and `wl_buffer` behind it, when it has any.
    pub(crate) link: Option<super::present_wayland::SlotLink>,
}

impl Slot {
    pub(crate) fn new(
        texture: wgpu::Texture,
        link: Option<super::present_wayland::SlotLink>,
    ) -> Self {
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
    pub(crate) wayland: Option<super::present_wayland::WaylandLink>,
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
        wayland: Option<super::present_wayland::WaylandLink>,
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
            wayland,
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
            if let Some(wayland) = &mut self.wayland {
                wayland.retire(old);
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
        if let Some(wayland) = &mut self.wayland {
            wayland.dispatch();
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
        loop {
            let free = self
                .slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| !slot.busy.load(Ordering::Acquire))
                .max_by_key(|(_, slot)| slot.painted.map_or(0, |frame| frame + 1))
                .map(|(index, _)| index);
            if free.is_some() {
                return free;
            }
            if self.slots.len() < self.limit {
                let slot = match &mut self.wayland {
                    Some(wayland) => match wayland.allocate(device, self.size) {
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
            // All of them are on screen or queued there. Wait for one back,
            // a while, and skip this frame if none comes: its damage is in
            // the history and reaches the next buffer drawn.
            let waited = self
                .wayland
                .as_mut()
                .is_some_and(|wayland| wayland.wait_release(deadline));
            if !waited {
                return None;
            }
        }
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
            if let Some(wayland) = &mut self.wayland {
                wayland.commit_without_buffer();
            }
            return;
        };
        let slot = &mut self.slots[index];
        slot.busy.store(true, Ordering::Release);
        self.presented = Some(index);
        let mut declared = std::mem::take(&mut self.undeclared);
        declared.extend_from_slice(damage);
        if let Some(wayland) = &mut self.wayland {
            wayland.present(queue, slot, &declared);
        }
    }

    /// Signals for the submission that finishes the pending buffer, when it
    /// needs any. Called just before it is submitted.
    pub(crate) fn before_submit(&mut self, queue: &wgpu::Queue) {
        if let (Some(index), Some(wayland)) = (self.pending, &mut self.wayland) {
            wayland.before_submit(queue, &self.slots[index]);
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
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        pixels
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u32, y: u32, width: u32, height: u32) -> DamageRect {
        DamageRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_new_buffer_is_painted_whole() {
        let mut history = DamageHistory::default();
        history.record(&[rect(0, 0, 4, 4)]);
        assert_eq!(repaint(&history, None, (100, 100)), None);
    }

    #[test]
    fn a_buffer_catches_up_on_every_frame_it_missed() {
        let mut history = DamageHistory::default();
        let first = history.record(&[rect(0, 0, 4, 4)]);
        history.record(&[rect(10, 10, 4, 4)]);
        history.record(&[rect(20, 20, 4, 4)]);
        let mut rects = repaint(&history, Some(first), (100, 100)).unwrap();
        rects.sort_by_key(|rect| rect.x);
        assert_eq!(rects, vec![rect(10, 10, 4, 4), rect(20, 20, 4, 4)]);
        // The buffer that showed the latest frame has nothing to catch up.
        assert_eq!(
            repaint(&history, Some(history.frame()), (100, 100)),
            Some(vec![])
        );
    }

    #[test]
    fn a_buffer_older_than_the_history_is_painted_whole() {
        let mut history = DamageHistory::default();
        let first = history.record(&[rect(0, 0, 1, 1)]);
        for _ in 0..=HISTORY {
            history.record(&[rect(1, 1, 1, 1)]);
        }
        assert_eq!(repaint(&history, Some(first), (10, 10)), None);
        assert!(repaint(&history, Some(first + 1), (10, 10)).is_some());
    }

    #[test]
    fn rectangles_inside_others_are_dropped_and_many_become_one() {
        let mut history = DamageHistory::default();
        let start = history.record(&[]);
        history.record(&[rect(0, 0, 50, 50), rect(10, 10, 5, 5)]);
        assert_eq!(
            repaint(&history, Some(start), (100, 100)),
            Some(vec![rect(0, 0, 50, 50)])
        );
        let start = history.record(&[]);
        let many: Vec<_> = (0..MAX_RECTS as u32 + 1)
            .map(|index| rect(index * 3, 0, 2, 2))
            .collect();
        history.record(&many);
        assert_eq!(
            repaint(&history, Some(start), (100, 100)),
            Some(vec![rect(0, 0, MAX_RECTS as u32 * 3 + 2, 2)])
        );
    }

    #[test]
    fn damage_past_the_edge_is_cut_to_the_buffer() {
        let mut history = DamageHistory::default();
        let start = history.record(&[]);
        history.record(&[rect(90, 90, 20, 20), rect(200, 200, 5, 5)]);
        assert_eq!(
            repaint(&history, Some(start), (100, 100)),
            Some(vec![rect(90, 90, 10, 10)])
        );
    }
}
