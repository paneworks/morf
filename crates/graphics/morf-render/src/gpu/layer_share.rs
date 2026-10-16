//! Packing layers side by side into shared textures, and blurred layers
//! sharing one target and one blur chain.
//!
//! Layers blurred by the same radius that do not depend on each other are
//! placed apart in one texture, drawn in one pass and blurred once; each is
//! then composited back by its own blit from the blurred texture. Each
//! keeps the blur it would have had in a texture of its own: places sit on
//! the grid of the chain's quarter level, and far enough apart that no tap of
//! the chain reaches a neighbour.

use crate::{DamageRect, DrawList, Layer};

use super::backend_types::WgpuBackend;
use super::targets::create_blur_chain;
use super::textures::BlurChain;

/// Gap left between layers in an atlas, so filtering never reaches a
/// neighbour.
const GUTTER: u32 = 2;

/// One packed atlas: its size, and where each rectangle — by index into what
/// was packed — went.
pub(crate) type Atlas = ((u32, u32), Vec<(usize, (u32, u32))>);

/// Places rectangles of `sizes` on shelves no wider than `width`, starting a
/// new atlas whenever one would grow past `height`. Returns, per atlas, its
/// size and where each rectangle — by index into `sizes` — went.
pub(crate) fn pack(sizes: &[(u32, u32)], limit: (u32, u32)) -> Vec<Atlas> {
    pack_spaced(sizes, limit, GUTTER, 2)
}

/// [`pack`], `gutter` apart and on a grid of `grid` texels.
pub(crate) fn pack_spaced(
    sizes: &[(u32, u32)],
    (width, height): (u32, u32),
    gutter: u32,
    grid: u32,
) -> Vec<Atlas> {
    let mut indices: Vec<usize> = (0..sizes.len()).collect();
    indices.sort_by_key(|index| std::cmp::Reverse(sizes[*index].1));
    let mut atlases = Vec::new();
    let mut placed: Vec<(usize, (u32, u32))> = Vec::new();
    let (mut x, mut y, mut shelf, mut used_width) = (0u32, 0u32, 0u32, 0u32);
    for index in indices {
        // Every place starts on the grid; see `layer_regions`.
        let (w, h) = (
            sizes[index].0.next_multiple_of(grid),
            sizes[index].1.next_multiple_of(grid),
        );
        if x > 0 && x + w > width {
            x = 0;
            y = (y + shelf + gutter).next_multiple_of(grid);
            shelf = 0;
        }
        if y > 0 && y + h > height {
            atlases.push(((used_width, y + shelf), std::mem::take(&mut placed)));
            (x, y, shelf, used_width) = (0, 0, 0, 0);
        }
        placed.push((index, (x, y)));
        used_width = used_width.max(x + w);
        shelf = shelf.max(h);
        x = (x + w + gutter).next_multiple_of(grid);
    }
    if !placed.is_empty() {
        atlases.push(((used_width, y + shelf), placed));
    }
    atlases
}

/// The blur, as bits, a layer can share a target and a chain by: one whose
/// composite is the blurred picture alone -- no shadow, no effect shader,
/// no mask.
fn shared_blur(layer: &Layer) -> Option<u32> {
    let alone = layer.shadow_color.alpha <= 0.0
        && layer.shader.is_none()
        && layer.alpha_mask.is_none()
        && layer.mask_for.is_none();
    (layer.blur > 0.0 && alone).then_some(layer.blur.to_bits())
}

/// Splits layers that read their own neighbourhood, all independent of
/// each other, into groups blurred by the same radius that share a target
/// and the rest, each alone.
pub(crate) fn group(list: &DrawList, layers: Vec<usize>) -> (Vec<Vec<usize>>, Vec<usize>) {
    let mut groups: Vec<(u32, Vec<usize>)> = Vec::new();
    let mut alone = Vec::new();
    for layer in layers {
        match shared_blur(&list.layers[layer]) {
            Some(bits) => match groups.iter_mut().find(|(seen, _)| *seen == bits) {
                Some((_, members)) => members.push(layer),
                None => groups.push((bits, vec![layer])),
            },
            None => alone.push(layer),
        }
    }
    let mut shared = Vec::new();
    for (_, members) in groups {
        if members.len() > 1 {
            shared.push(members);
        } else {
            alone.extend(members);
        }
    }
    (shared, alone)
}

/// How far apart blurred layers sit in a shared target, in texels. The
/// chain at `offset` reaches about fifteen offsets and nine texels; a
/// region keeps eight offsets of margin around what it draws, and the rest
/// is slack for glyphs that spill past their boxes.
pub(crate) fn blur_gutter(offset: f32) -> u32 {
    (offset * 8.0).ceil() as u32 + 16
}

/// A texture, its view, and where in it one layer's region starts.
pub(crate) type Placement = (wgpu::Texture, wgpu::TextureView, (u32, u32));

/// One layer's place in a shared blurred target: where, the bind group
/// that samples the blurred texture, and for the first layer of the target
/// the chain that blurs it.
pub(crate) struct SharedBlur {
    pub(crate) placement: Placement,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) chain: Option<BlurChain>,
}

impl WgpuBackend {
    /// Places `layers`, blurred by the same radius, in shared targets and
    /// makes each target's chain. A region the surface cut short is not
    /// shared: alone, its blur clamps at the cut as it always has.
    pub(crate) fn share_blurred(
        &mut self,
        list: &DrawList,
        layers: &[usize],
        regions: &[Option<super::layer_pool::LayerRegion>],
        scale: f64,
    ) -> Vec<(usize, SharedBlur)> {
        let whole = |layer: usize| {
            let unclipped = super::layer_pool::layer_extent(
                &list.layers[layer],
                (scale * 120.0).round() as u32,
                DamageRect {
                    x: 0,
                    y: 0,
                    width: u32::MAX / 4,
                    height: u32::MAX / 4,
                },
            );
            regions[layer].as_ref().map(|region| region.bounds) == unclipped
        };
        let layers: Vec<usize> = layers
            .iter()
            .copied()
            .filter(|layer| whole(*layer))
            .collect();
        if layers.len() < 2 {
            return Vec::new();
        }
        let offset = (list.layers[layers[0]].blur * scale as f32 / 4.0).max(0.5);
        let sizes: Vec<(u32, u32)> = layers
            .iter()
            .map(|layer| {
                regions[*layer]
                    .as_ref()
                    .map_or((0, 0), |r| (r.bounds.width, r.bounds.height))
            })
            .collect();
        let format = super::target_format(self.space);
        let limit = (self.width, self.height);
        let largest = self.device.limits().max_texture_dimension_2d;
        let mut shared = Vec::new();
        for (size, placed) in
            pack_spaced(&sizes, (self.width.max(1), largest), blur_gutter(offset), 4)
        {
            let size = (size.0.next_multiple_of(4), size.1.next_multiple_of(4));
            let (texture, view) = self.layer_pool.take(
                &self.device,
                format,
                size,
                true,
                (self.width.max(1), largest),
            );
            let pool = &mut self.layer_pool;
            let device = &self.device;
            let chain = create_blur_chain(
                device,
                &self.blur_layout,
                &self.blur_sampler,
                (&texture, &view),
                offset,
                |size| pool.take(device, format, size, true, limit),
            );
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("morf shared blur bind group"),
                layout: &self.glyph_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&chain.views[3]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.glyph_sampler),
                    },
                ],
            });
            let mut chain = Some(chain);
            for (index, origin) in placed {
                shared.push((
                    layers[index],
                    SharedBlur {
                        placement: (texture.clone(), view.clone(), origin),
                        bind_group: bind_group.clone(),
                        chain: chain.take(),
                    },
                ));
            }
        }
        shared
    }
}
