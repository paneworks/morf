use super::{backend_types::*, glyphs::*, targets::*, textures::*};
use crate::DrawList;
use crate::effects::color_array;

/// A quad over physical pixels `(x, y, width, height)` of a surface
/// `target_size` big, in clip space.
fn physical_quad(
    (x, y, width, height): (f64, f64, f64, f64),
    (target_width, target_height): (u32, u32),
) -> ([f32; 2], [f32; 4]) {
    let (target_width, target_height) = (f64::from(target_width), f64::from(target_height));
    (
        [
            (x / target_width * 2.0 - 1.0) as f32,
            (1.0 - y / target_height * 2.0) as f32,
        ],
        [
            (width / target_width * 2.0) as f32,
            0.0,
            0.0,
            (-height / target_height * 2.0) as f32,
        ],
    )
}

/// A layer's texture, its view, and where in it the layer's region starts.
type Placement = (wgpu::Texture, wgpu::TextureView, (u32, u32));

impl WgpuBackend {
    /// Takes a target for every layer something reads this frame and prepares
    /// the quad that composites it back.
    ///
    /// `regions` says which part of the surface each layer has to hold, and
    /// `stages` which layers share an atlas; see `layer_pool`. A layer without
    /// a region gets no target and is not drawn.
    ///
    /// Split out of `render` because it is self-contained and long: a layer's
    /// blur chain, its shadow chain, its mask and the instance that draws it
    /// are all decided here, and none of it interacts with the command loop
    /// that follows.
    pub(crate) fn build_layer_targets(
        &mut self,
        list: &DrawList,
        texture_batch: &mut TextureBatch,
        scale: f64,
        regions: &[Option<super::layer_pool::LayerRegion>],
        stages: &[super::layer_pool::Stage],
    ) -> Vec<Option<LayerTarget>> {
        use super::layer_pool::{LayerRegion, Stage, pack};
        let format = super::target_format(self.blend);
        let limit = (self.width, self.height);
        let largest = self.device.limits().max_texture_dimension_2d;
        // Where each layer's region lives: which texture, and where in it.
        let mut placements: Vec<Option<Placement>> = vec![None; list.layers.len()];
        for stage in stages {
            match stage {
                Stage::Backdrop(_) => {}
                Stage::Solo(layer) => {
                    let Some(region) = regions[*layer].as_ref().map(|region| region.bounds) else {
                        continue;
                    };
                    let (texture, view) = self.layer_pool.take(
                        &self.device,
                        format,
                        (region.width, region.height),
                        true,
                        limit,
                    );
                    placements[*layer] = Some((texture, view, (0, 0)));
                }
                Stage::Atlas(layers) => {
                    let sizes: Vec<(u32, u32)> = layers
                        .iter()
                        .map(|layer| {
                            regions[*layer]
                                .as_ref()
                                .map_or((0, 0), |r| (r.bounds.width, r.bounds.height))
                        })
                        .collect();
                    // Every atlas of one stage is drawn in the same pass, so a
                    // stage too large for one texture takes the first and the
                    // rest are rendered in passes of their own.
                    for (size, placed) in pack(&sizes, (self.width.max(1), largest)) {
                        let (texture, view) = self.layer_pool.take(
                            &self.device,
                            format,
                            size,
                            false,
                            (self.width.max(1), largest),
                        );
                        for (index, origin) in placed {
                            placements[layers[index]] =
                                Some((texture.clone(), view.clone(), origin));
                        }
                    }
                }
            }
        }
        let mut layer_targets = Vec::with_capacity(list.layers.len());
        // Layers sharing a texture share the bind group that samples it.
        let mut bind_groups: Vec<(wgpu::Texture, wgpu::BindGroup)> = Vec::new();
        for ((layer, region), placement) in list.layers.iter().zip(regions).zip(placements) {
            let (
                Some(LayerRegion {
                    bounds: region,
                    reads,
                }),
                Some((texture, view, at)),
            ) = (region.clone(), placement)
            else {
                layer_targets.push(None);
                continue;
            };
            let mut chain = |radius: f32| {
                let pool = &mut self.layer_pool;
                let device = &self.device;
                create_blur_chain(
                    device,
                    &self.blur_layout,
                    &self.blur_sampler,
                    (&texture, &view),
                    (radius * scale as f32 / 4.0).max(0.5),
                    |size| pool.take(device, format, size, true, limit),
                )
            };
            let blur = (layer.blur > 0.0).then(|| chain(layer.blur));
            let shadow = (layer.shadow_color.alpha > 0.0 && layer.shadow_blur > 0.0)
                .then(|| chain(layer.shadow_blur));
            let composite_view = blur.as_ref().map_or(&view, |chain| &chain.views[3]);
            let shared = blur
                .is_none()
                .then(|| {
                    bind_groups
                        .iter()
                        .find(|(seen, _)| *seen == texture)
                        .map(|(_, group)| group.clone())
                })
                .flatten();
            let bind_group = shared.unwrap_or_else(|| {
                let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("morf layer bind group"),
                    layout: &self.glyph_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(composite_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.glyph_sampler),
                        },
                    ],
                });
                if blur.is_none() {
                    bind_groups.push((texture.clone(), group.clone()));
                }
                group
            });
            let shadow_bind_group = (layer.shadow_color.alpha > 0.0).then(|| {
                let shadow_view = shadow.as_ref().map_or(&view, |chain| &chain.views[3]);
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("morf layer shadow bind group"),
                    layout: &self.glyph_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(shadow_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.glyph_sampler),
                        },
                    ],
                })
            });
            let placed = (
                f64::from(region.x),
                f64::from(region.y),
                f64::from(region.width),
                f64::from(region.height),
            );
            // The target holds the region at `at`; a pooled texture may
            // be larger than that, and an atlas holds other layers beside it.
            let (texture_width, texture_height) = (texture.width() as f32, texture.height() as f32);
            let uv = [
                at.0 as f32 / texture_width,
                at.1 as f32 / texture_height,
                region.width as f32 / texture_width,
                region.height as f32 / texture_height,
            ];
            let shadow_instance = shadow_bind_group.as_ref().map(|_| {
                let instance = texture_batch.instances.len() as u32;
                let (origin, axes) = physical_quad(
                    (
                        placed.0 + f64::from(layer.shadow_offset[0]) * scale,
                        placed.1 + f64::from(layer.shadow_offset[1]) * scale,
                        placed.2,
                        placed.3,
                    ),
                    (self.width, self.height),
                );
                texture_batch.instances.push(GlyphInstance {
                    origin,
                    axes,
                    uv,
                    color: [1.0, 1.0, 1.0, layer.shadow_color.alpha * layer.opacity],
                    color_overlay: {
                        let mut color = color_array(layer.shadow_color);
                        color[3] = 1.0;
                        color
                    },
                    mode: [0.0; 4],
                    ..GlyphInstance::default()
                });
                instance
            });
            let instance = texture_batch.instances.len() as u32;
            let (mask_enabled, mask_bounds, mask_inverse_0, mask_inverse_1, mask_radii) =
                layer_mask_data(layer.mask);
            let (origin, axes) = physical_quad(placed, (self.width, self.height));
            texture_batch.instances.push(GlyphInstance {
                origin,
                axes,
                uv,
                color: [1.0, 1.0, 1.0, layer.opacity],
                color_overlay: [0.0; 4],
                mode: [1.0, mask_enabled, 0.0, 0.0],
                // The mask is evaluated in logical surface coordinates.
                surface: [
                    (placed.0 / scale) as f32,
                    (placed.1 / scale) as f32,
                    (placed.2 / scale) as f32,
                    (placed.3 / scale) as f32,
                ],
                mask_bounds,
                mask_inverse_0,
                mask_inverse_1,
                mask_radii,
                ..GlyphInstance::default()
            });
            layer_targets.push(Some(LayerTarget {
                texture,
                view,
                region,
                origin: at,
                reads,
                bind_group,
                instance,
                blur,
                shadow_bind_group,
                shadow_instance,
                shadow,
            }));
        }
        layer_targets
    }
}
