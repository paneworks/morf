//! The backend's side of backdrops: deciding each frame which ones blur
//! again, and the textures and passes each one keeps.

use morf_layout::{Geometry, Transform2D};

use super::super::backend_types::WgpuBackend;
use super::super::glyphs::{GlyphInstance, layer_mask_data, transformed_quad};
use super::super::targets::{clamp_scissor, create_blur_pass};
use super::super::textures::TextureBatch;
use super::{BackdropDraw, BackdropEntry, Beneath, BeneathOp, ChildLayers, beneath_ops};
use crate::backdrop::{backdrop_plan, level_sizes, pass_order};
use crate::effects::physical_damage;
use crate::{DamageRect, DrawCommand, DrawList, Layer, LayerMask};

impl WgpuBackend {
    /// Whether the last frame was skipped for want of a buffer, cleared by
    /// asking. The caller owes the surface a paint.
    pub fn take_skipped(&mut self) -> bool {
        std::mem::take(&mut self.skipped)
    }

    /// Decides, for every backdrop in the list, whether it blurs again this
    /// frame, and adds the quad that draws it. Returns one slot per command.
    ///
    /// Runs in paint order, so a backdrop over another one sees the lower
    /// one's new generation and blurs again with it.
    pub(crate) fn prepare_backdrops(
        &mut self,
        list: &DrawList,
        texture_batch: &mut TextureBatch,
        (command_layers, child_layers): (&[Option<usize>], &ChildLayers),
        scale_120: u32,
    ) -> Vec<Option<BackdropDraw>> {
        let scale = f64::from(scale_120.max(1)) / 120.0;
        let mut draws: Vec<Option<BackdropDraw>> = Vec::new();
        draws.resize_with(list.commands.len(), || None);
        for entry in self.backdrops.entries.values_mut() {
            entry.used = false;
        }
        for (index, command) in list.commands.iter().enumerate() {
            let DrawCommand::Backdrop {
                node,
                bounds,
                transform,
                radii,
                radius,
                saturation,
                ..
            } = command
            else {
                continue;
            };
            let Some(reach) = command.backdrop_reach() else {
                continue;
            };
            let Some(region) = physical_damage(reach, scale_120)
                .and_then(|region| clamp_scissor(region, self.width, self.height))
                .map(|(x, y, width, height)| DamageRect {
                    x,
                    y,
                    width,
                    height,
                })
            else {
                continue;
            };
            let plan = backdrop_plan(radius * scale);
            let ops = beneath_ops(list, command_layers, child_layers, index, reach);
            let beneath = self.beneath_of(list, texture_batch, &ops);
            let stale = match self.backdrops.entries.get(node) {
                Some(entry) => {
                    entry.region != region || entry.plan != plan || entry.beneath != beneath
                }
                None => true,
            };
            if stale {
                let reuse = self
                    .backdrops
                    .entries
                    .get(node)
                    .is_some_and(|entry| entry.region == region && entry.plan == plan);
                if reuse {
                    let entry = self.backdrops.entries.get_mut(node).expect("just found");
                    entry.beneath = beneath;
                    entry.generation += 1;
                } else {
                    let generation = self
                        .backdrops
                        .entries
                        .get(node)
                        .map_or(0, |entry| entry.generation + 1);
                    let entry = self.backdrop_entry(region, plan, beneath, generation);
                    self.backdrops.entries.insert(*node, entry);
                }
                self.backdrops.blurs += 1;
            }
            let entry = self.backdrops.entries.get_mut(node).expect("made above");
            entry.used = true;
            let logical = Geometry {
                x: f64::from(region.x) / scale,
                y: f64::from(region.y) / scale,
                width: f64::from(region.width) / scale,
                height: f64::from(region.height) / scale,
            };
            let (origin, axes) = transformed_quad(
                Transform2D::IDENTITY,
                logical,
                scale,
                (self.width, self.height),
            );
            let (mask_enabled, mask_bounds, mask_inverse_0, mask_inverse_1, mask_radii) =
                layer_mask_data(Some(LayerMask {
                    bounds: *bounds,
                    transform: *transform,
                    radii: *radii,
                }));
            let instance = texture_batch.instances.len() as u32;
            texture_batch.instances.push(GlyphInstance {
                origin,
                axes,
                uv: [0.0, 0.0, 1.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
                color_overlay: [0.0; 4],
                // A layer's composite, marked as a backdrop so the shader
                // applies the saturation carried in `field`.
                mode: [2.0, mask_enabled, 0.0, 0.0],
                surface: [
                    logical.x as f32,
                    logical.y as f32,
                    logical.width as f32,
                    logical.height as f32,
                ],
                mask_bounds,
                mask_inverse_0,
                mask_inverse_1,
                mask_radii,
                field: [*saturation as f32, 0.0, 0.0, 0.0],
                ..GlyphInstance::default()
            });
            draws[index] = Some(BackdropDraw {
                node: *node,
                instance,
                refresh: stale.then_some(ops),
            });
        }
        // A backdrop that was not drawn this frame has gone; its textures go
        // with it.
        self.backdrops.entries.retain(|_, entry| entry.used);
        draws
    }

    /// Everything a backdrop drawn from `ops` depends on.
    fn beneath_of(
        &self,
        list: &DrawList,
        texture_batch: &TextureBatch,
        ops: &[BeneathOp],
    ) -> Vec<Beneath> {
        let mut beneath = Vec::new();
        let command = |index: usize, beneath: &mut Vec<Beneath>| {
            let command = &list.commands[index];
            let drawn = match command {
                DrawCommand::Texture { .. } => texture_batch.command_instances[index].is_some(),
                _ => true,
            };
            beneath.push(Beneath::Command(command.clone(), drawn));
            if let DrawCommand::Backdrop { node, .. } = command {
                let generation = self
                    .backdrops
                    .entries
                    .get(node)
                    .map_or(u64::MAX, |entry| entry.generation);
                beneath.push(Beneath::Backdrop(*node, generation));
            }
        };
        for op in ops {
            match *op {
                BeneathOp::Command(index) => command(index, &mut beneath),
                BeneathOp::Layer(layer) => {
                    let layer = &list.layers[layer];
                    // Where the layer sits in the list is not part of how it
                    // looks: a line of text added elsewhere shifts every range.
                    beneath.push(Beneath::Layer(Layer {
                        commands: 0..0,
                        parent: None,
                        ..layer.clone()
                    }));
                    for index in layer.commands.clone() {
                        command(index, &mut beneath);
                    }
                }
            }
        }
        beneath
    }

    /// Textures and passes for a backdrop over `region`.
    fn backdrop_entry(
        &self,
        region: DamageRect,
        (levels, offset): (u32, f32),
        beneath: Vec<Beneath>,
        generation: u64,
    ) -> BackdropEntry {
        let format = super::super::target_format(self.blend);
        let sizes = level_sizes(region.width, region.height, levels);
        let texture = |(width, height): (u32, u32), label: &str, copy: bool| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | if copy {
                        wgpu::TextureUsages::COPY_DST
                    } else {
                        wgpu::TextureUsages::empty()
                    },
                view_formats: &[],
            })
        };
        let source = texture(sizes[0], "morf backdrop source", true);
        let source_view = source.create_view(&wgpu::TextureViewDescriptor::default());
        let levels_made: Vec<(wgpu::Texture, wgpu::TextureView)> = sizes[1..]
            .iter()
            .map(|size| {
                let level = texture(*size, "morf backdrop level", false);
                let view = level.create_view(&wgpu::TextureViewDescriptor::default());
                (level, view)
            })
            .collect();
        let view_of = |level: usize| {
            if level == 0 {
                &source_view
            } else {
                &levels_made[level - 1].1
            }
        };
        let passes = pass_order(levels)
            .into_iter()
            .map(|(from, to)| {
                (
                    create_blur_pass(
                        &self.device,
                        &self.blur_layout,
                        &self.blur_sampler,
                        view_of(from),
                        sizes[from],
                        offset,
                        if to < from { 1.0 } else { 0.0 },
                    ),
                    to,
                )
            })
            .collect();
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("morf backdrop bind group"),
            layout: &self.glyph_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view_of(1)),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.glyph_sampler),
                },
            ],
        });
        BackdropEntry {
            region,
            plan: (levels, offset),
            beneath,
            generation,
            source,
            levels: levels_made,
            passes,
            bind_group,
            used: true,
        }
    }

    /// The full-surface target backdrops re-render what is beneath them into.
    pub(crate) fn backdrop_scratch(&mut self) -> (wgpu::Texture, wgpu::TextureView) {
        if self.backdrops.scratch.is_none() {
            let format = super::super::target_format(self.blend);
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("morf backdrop scratch"),
                size: wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.backdrops.scratch = Some((texture, view));
        }
        self.backdrops.scratch.clone().expect("made above")
    }

    /// How many times a backdrop has blurred since the backend started.
    ///
    /// For measuring: a still frame should add nothing to it.
    pub fn backdrop_blurs(&self) -> u64 {
        self.backdrops.blurs
    }
}
