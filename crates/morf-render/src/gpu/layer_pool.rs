//! Offscreen layer targets sized to what a frame reads of them.
//!
//! Every rounded clip, opacity group, rotation, shadow and blur renders its
//! subtree into a target of its own and composites it back. Those targets used
//! to be the size of the surface and were cleared and redrawn in full every
//! frame, whatever changed: a clock ticking inside ten rounded panels on a
//! 1920x1080 desk cleared and refilled ten full-screen textures sixty times a
//! second, which on an integrated GPU is most of a frame.
//!
//! A layer's pixels are only ever read where its composite is drawn, and that
//! is clipped to the frame's damage. So each layer is now rendered only over
//! the part of it some reader will sample this frame — the damage it
//! composites through, what its parent will read of it, and what a frosted
//! backdrop re-rendering beneath itself will read of it — into a pooled
//! texture just big enough for that. A layer the damage does not touch is not
//! rendered at all.
//!
//! A layer that reads its own neighbourhood — a blur, a drop shadow, an effect
//! shader — is the exception: whatever part of it is read, the whole of it
//! contributes, so it is rendered whole, into a texture exactly its size.
//!
//! The rest are packed side by side into shared atlases, one render pass per
//! atlas rather than one per layer. A render pass is not free on an integrated
//! GPU — the target changes, caches are flushed, the texture is made readable
//! again — and thirty small widgets were thirty of them, which cost more than
//! everything they drew. Layers that do not depend on each other share a pass:
//! everything in one stretch of offscreen work between two backdrops that blur
//! again, at the same depth of nesting counted from the innermost.

use morf_layout::Geometry;

use crate::effects::physical_damage;
use crate::{DamageRect, DrawList, Layer};

use super::targets::{intersect_damage, union_damage as union};

/// Unused pooled textures are dropped after this many frames without a use.
const IDLE_FRAMES: u32 = 120;

// Animated blur/shadow targets need exact sizes, so every frame can leave a
// different large texture behind. Age alone allowed hundreds of MiB per
// output to accumulate, and an idle shell might never draw 120 more frames.
// Bound spare storage independently of the live frame's required targets.
const SPARE_PIXELS: u64 = 8 * 1024 * 1024; // 32 MiB of RGBA8 per renderer.

/// Oldest unused targets to release to bring spare storage under its budget.
pub(crate) fn spare_evictions(mut unused: Vec<(usize, u64, u32)>, budget: u64) -> Vec<usize> {
    let mut pixels: u64 = unused.iter().map(|(_, pixels, _)| *pixels).sum();
    if pixels <= budget {
        return Vec::new();
    }
    // Reuse the freshest targets first; among equally old targets, releasing
    // the largest gets under budget with the fewest cache misses.
    unused.sort_unstable_by_key(|(index, pixels, idle)| {
        (std::cmp::Reverse(*idle), std::cmp::Reverse(*pixels), *index)
    });
    let mut evicted = Vec::new();
    for (index, size, _) in unused {
        if pixels <= budget {
            break;
        }
        pixels -= size;
        evicted.push(index);
    }
    evicted.sort_unstable();
    evicted
}

/// Pooled textures are allocated in steps of this many pixels, so a layer that
/// grows by a pixel a frame does not allocate a texture a frame.
const SIZE_STEP: u32 = 64;

/// Whether a layer's composite reads pixels other than the one it writes.
pub(crate) fn samples_neighbours(layer: &Layer) -> bool {
    layer.blur > 0.0 || layer.shadow_color.alpha > 0.0 || layer.shader.is_some()
}

fn grow(rect: DamageRect, by: u32, surface: DamageRect) -> DamageRect {
    let x = rect.x.saturating_sub(by);
    let y = rect.y.saturating_sub(by);
    let grown = DamageRect {
        x,
        y,
        width: rect.x + rect.width + by - x,
        height: rect.y + rect.height + by - y,
    };
    intersect_damage(grown, surface).unwrap_or(rect)
}

/// Everything a layer can put on the surface, in physical pixels: where it is
/// composited, and for a shadow also where the shadow is taken from.
///
/// An effect shader is given the whole surface, as it always was: it sees the
/// layer in surface coordinates and may sample anywhere in them.
fn layer_extent(layer: &Layer, scale_120: u32, surface: DamageRect) -> Option<DamageRect> {
    if layer.shader.is_some() {
        return Some(surface);
    }
    let mut extent = physical_damage(layer.bounds, scale_120)?;
    if layer.shadow_color.alpha > 0.0 {
        // The shadow is the layer's own texture drawn again at an offset, so
        // the texture has to hold the unshifted picture too.
        let unshifted = Geometry {
            x: layer.bounds.x - f64::from(layer.shadow_offset[0]),
            y: layer.bounds.y - f64::from(layer.shadow_offset[1]),
            ..layer.bounds
        };
        if let Some(unshifted) = physical_damage(unshifted, scale_120) {
            extent = union(extent, unshifted);
        }
    }
    if samples_neighbours(layer) {
        // On a four-pixel grid, so the blur's half and quarter levels sample
        // the same grid a full-surface target would have.
        let x = extent.x / 4 * 4;
        let y = extent.y / 4 * 4;
        extent = DamageRect {
            x,
            y,
            width: (extent.x + extent.width - x).div_ceil(4) * 4,
            height: (extent.y + extent.height - y).div_ceil(4) * 4,
        };
    }
    intersect_damage(extent, surface)
}

/// Most rectangles a layer is rendered through before they are merged into
/// the one that holds them all.
const MAX_READS: usize = 64;

/// What of one layer has to be rendered this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LayerRegion {
    /// Everything the target holds, in surface pixels.
    pub(crate) bounds: DamageRect,
    /// The rectangles inside it that are drawn: a layer read through thirty
    /// small damage rectangles is drawn through those, not through the one
    /// that spans them.
    pub(crate) reads: Vec<DamageRect>,
}

/// The part of each layer, in surface pixels, that has to be rendered this
/// frame; `None` for a layer nothing reads.
///
/// `backdrop_reads` are the regions frosted backdrops re-render beneath
/// themselves this frame, with the layers each one composites there.
pub(crate) fn layer_regions(
    list: &DrawList,
    damage: &[DamageRect],
    backdrop_reads: &[(DamageRect, Vec<usize>)],
    scale_120: u32,
    (width, height): (u32, u32),
) -> Vec<Option<LayerRegion>> {
    let surface = DamageRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let mut wanted: Vec<Vec<DamageRect>> = vec![Vec::new(); list.layers.len()];
    let bounds: Vec<Option<DamageRect>> = list
        .layers
        .iter()
        .map(|layer| physical_damage(layer.bounds, scale_120))
        .collect();
    let want = |wanted: &mut Vec<Vec<DamageRect>>, layer: usize, read: DamageRect| {
        if let Some(read) = bounds[layer].and_then(|bounds| intersect_damage(read, bounds)) {
            wanted[layer].push(read);
        }
    };
    for (region, layers) in backdrop_reads {
        for &layer in layers {
            want(&mut wanted, layer, *region);
        }
    }
    let mut regions: Vec<Option<LayerRegion>> = vec![None; list.layers.len()];
    for (index, layer) in list.layers.iter().enumerate() {
        // A mask holds exactly what the layer it masks holds, pixel for
        // pixel: the composite reads both at one point of one quad. Even an
        // empty one, which is then a cleared region, and masks everything.
        if let Some(owner) = layer.mask_for {
            if owner < index {
                regions[index] = regions[owner].clone();
            }
            continue;
        }
        if layer.commands.is_empty() {
            continue;
        }
        match layer.parent {
            None => {
                for rect in damage {
                    want(&mut wanted, index, *rect);
                }
            }
            // A parent is always listed before what it contains, so its
            // region is settled by now; anything else is rendered whole.
            Some(parent) if parent < index => {
                let reads = regions[parent]
                    .as_ref()
                    .map(|region| region.reads.clone())
                    .unwrap_or_default();
                for read in reads {
                    want(&mut wanted, index, read);
                }
            }
            Some(_) => want(&mut wanted, index, surface),
        }
        if wanted[index].is_empty() {
            continue;
        }
        let Some(extent) = layer_extent(layer, scale_120, surface) else {
            continue;
        };
        let reads = if samples_neighbours(layer) {
            vec![extent]
        } else {
            // A pixel of margin: the composite samples texel centres, and a
            // rounding error must find the neighbour it would have found in a
            // full-surface target rather than a cleared one.
            let reads = crate::effects::merge_damage(
                std::mem::take(&mut wanted[index])
                    .into_iter()
                    .filter_map(|read| intersect_damage(grow(read, 1, surface), extent))
                    .collect(),
            );
            if reads.len() > MAX_READS {
                vec![reads.iter().copied().reduce(union).expect("not empty")]
            } else {
                reads
            }
        };
        let Some(mut bounds) = reads.iter().copied().reduce(union) else {
            continue;
        };
        // On even pixels, and placed at even texels: derivatives are taken
        // over two-by-two blocks of the target, and an antialiased edge drawn
        // with its blocks split differently comes out a step different.
        let (x, y) = (bounds.x & !1, bounds.y & !1);
        bounds = DamageRect {
            x,
            y,
            width: bounds.x + bounds.width - x,
            height: bounds.y + bounds.height - y,
        };
        regions[index] = Some(LayerRegion { bounds, reads });
    }
    regions
}

/// Gap left between layers in an atlas, so filtering never reaches a
/// neighbour.
const GUTTER: u32 = 2;

/// One piece of offscreen work, in the order it is done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    /// A backdrop blurring again what is beneath it, at this command.
    Backdrop(usize),
    /// Layers rendered side by side into one atlas, in one pass.
    Atlas(Vec<usize>),
    /// A layer that reads its own neighbourhood, into a texture of its own.
    Solo(usize),
}

/// Groups the offscreen work so layers that do not depend on each other share
/// a pass.
///
/// `order` is paint order with every layer after what it contains; `targets`
/// says which layers are drawn at all, and `refreshes` which backdrops blur
/// again. A backdrop that does not blur again does no work and orders nothing.
/// Between two that do, a layer depends only on the layers it contains, so
/// the layers are grouped by how deep their nesting goes beneath them: the
/// innermost first, then what holds them, and so on.
pub(crate) fn schedule(
    list: &DrawList,
    order: &[super::backdrops::Offscreen],
    targets: &[Option<DamageRect>],
    refreshes: impl Fn(usize) -> bool,
) -> Vec<Stage> {
    use super::backdrops::Offscreen;
    let mut stages = Vec::new();
    let mut height = vec![0usize; list.layers.len()];
    let mut segment: Vec<usize> = Vec::new();
    let flush = |segment: &mut Vec<usize>, height: &mut Vec<usize>, stages: &mut Vec<Stage>| {
        let deepest = segment.iter().map(|layer| height[*layer]).max();
        for level in 0..=deepest.unwrap_or(0) {
            let (solo, shared): (Vec<usize>, Vec<usize>) = segment
                .iter()
                .copied()
                .filter(|layer| height[*layer] == level)
                .partition(|layer| samples_neighbours(&list.layers[*layer]));
            if !shared.is_empty() {
                stages.push(Stage::Atlas(shared));
            }
            stages.extend(solo.into_iter().map(Stage::Solo));
        }
        // What is done is done: a parent in a later stretch waits for
        // nothing here.
        for layer in segment.drain(..) {
            height[layer] = 0;
            if let Some(parent) = list.layers[layer].parent
                && parent < height.len()
            {
                height[parent] = 0;
            }
        }
    };
    for step in order {
        match *step {
            Offscreen::Backdrop(command) => {
                if refreshes(command) {
                    flush(&mut segment, &mut height, &mut stages);
                    stages.push(Stage::Backdrop(command));
                }
            }
            Offscreen::Layer(layer) => {
                if targets[layer].is_none() {
                    continue;
                }
                // Everything this layer holds came before it; whatever of that
                // is in this stretch has its height by now.
                segment.push(layer);
                if let Some(parent) = list.layers[layer].parent
                    && parent < height.len()
                {
                    height[parent] = height[parent].max(height[layer] + 1);
                }
            }
        }
    }
    flush(&mut segment, &mut height, &mut stages);
    stages
}

/// One packed atlas: its size, and where each rectangle — by index into what
/// was packed — went.
pub(crate) type Atlas = ((u32, u32), Vec<(usize, (u32, u32))>);

/// Places rectangles of `sizes` on shelves no wider than `width`, starting a
/// new atlas whenever one would grow past `height`. Returns, per atlas, its
/// size and where each rectangle — by index into `sizes` — went.
pub(crate) fn pack(sizes: &[(u32, u32)], (width, height): (u32, u32)) -> Vec<Atlas> {
    let mut indices: Vec<usize> = (0..sizes.len()).collect();
    indices.sort_by_key(|index| std::cmp::Reverse(sizes[*index].1));
    let mut atlases = Vec::new();
    let mut placed: Vec<(usize, (u32, u32))> = Vec::new();
    let (mut x, mut y, mut shelf, mut used_width) = (0u32, 0u32, 0u32, 0u32);
    for index in indices {
        // Every place starts on an even texel; see `layer_regions`.
        let (w, h) = (
            sizes[index].0.next_multiple_of(2),
            sizes[index].1.next_multiple_of(2),
        );
        if x > 0 && x + w > width {
            x = 0;
            y += shelf + GUTTER;
            shelf = 0;
        }
        if y > 0 && y + h > height {
            atlases.push(((used_width, y + shelf), std::mem::take(&mut placed)));
            (x, y, shelf, used_width) = (0, 0, 0, 0);
        }
        placed.push((index, (x, y)));
        used_width = used_width.max(x + w);
        shelf = shelf.max(h);
        x += w + GUTTER;
    }
    if !placed.is_empty() {
        atlases.push(((used_width, y + shelf), placed));
    }
    atlases
}

struct Pooled {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    idle: u32,
    taken: bool,
}

/// Offscreen textures kept between frames and handed to whichever layer fits.
#[derive(Default)]
pub(crate) struct LayerPool {
    entries: Vec<Pooled>,
}

impl LayerPool {
    /// Makes every texture available again for a new frame.
    pub(crate) fn begin_frame(&mut self) {
        for entry in &mut self.entries {
            entry.taken = false;
        }
    }

    /// Ages what this frame did not use and drops what has sat idle too long.
    pub(crate) fn end_frame(&mut self) {
        for entry in &mut self.entries {
            if !entry.taken {
                entry.idle += 1;
            }
        }
        self.entries.retain(|entry| entry.idle <= IDLE_FRAMES);
        let unused = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                (!entry.taken).then(|| {
                    (
                        index,
                        u64::from(entry.texture.width()) * u64::from(entry.texture.height()),
                        entry.idle,
                    )
                })
            })
            .collect();
        let evicted = spare_evictions(unused, SPARE_PIXELS);
        for index in evicted.into_iter().rev() {
            self.entries.swap_remove(index);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    /// A texture at least `width` by `height` — exactly that size when
    /// `exact`, for a target a blur chain reads whole — not yet used this
    /// frame. Its contents are whatever was last drawn into it.
    pub(crate) fn take(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        (width, height): (u32, u32),
        exact: bool,
        limit: (u32, u32),
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let fits = |entry: &Pooled| {
            let (w, h) = (entry.texture.width(), entry.texture.height());
            !entry.taken
                && entry.texture.format() == format
                && if exact {
                    w == width && h == height
                } else {
                    w >= width && h >= height
                }
        };
        let best = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| fits(entry))
            .min_by_key(|(_, entry)| {
                u64::from(entry.texture.width()) * u64::from(entry.texture.height())
            })
            .map(|(index, _)| index);
        let index = match best {
            Some(index) => index,
            None => {
                let size = if exact {
                    (width, height)
                } else {
                    let step = |wanted: u32, limit: u32| {
                        wanted.next_multiple_of(SIZE_STEP).min(limit).max(wanted)
                    };
                    (step(width, limit.0), step(height, limit.1))
                };
                let (texture, view) = super::targets::create_target(device, size.0, size.1, format);
                self.entries.push(Pooled {
                    texture,
                    view,
                    idle: 0,
                    taken: false,
                });
                self.entries.len() - 1
            }
        };
        let entry = &mut self.entries[index];
        entry.taken = true;
        entry.idle = 0;
        (entry.texture.clone(), entry.view.clone())
    }

    /// How many textures the pool holds, and their pixels, for measuring.
    #[cfg(test)]
    pub(crate) fn footprint(&self) -> (usize, u64) {
        (
            self.entries.len(),
            self.entries
                .iter()
                .map(|entry| u64::from(entry.texture.width()) * u64::from(entry.texture.height()))
                .sum(),
        )
    }
}
