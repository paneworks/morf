//! The passes of a frame: the offscreen stages -- what is beneath a backdrop,
//! and the layers rendered to textures -- and the surface pass over the damage.

use crate::DamageRect;

use super::super::{backdrops::ChildLayers, layer_pool::Stage, targets::*};
use super::frame_draw::FrameDraw;

impl FrameDraw<'_> {
    /// The whole surface, at the origin of its own target.
    fn surface_frame(&self) -> (DamageRect, (u32, u32)) {
        (
            DamageRect {
                x: 0,
                y: 0,
                width: self.backend.width,
                height: self.backend.height,
            },
            (0, 0),
        )
    }

    /// Encodes the offscreen stages, in the order the layer pool scheduled.
    pub(super) fn encode_stages(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        stages: &[Stage],
        backdrop_scratch: Option<&(wgpu::Texture, wgpu::TextureView)>,
        command_layers: &[Option<usize>],
        child_layers: &ChildLayers,
    ) {
        for stage in stages {
            let layers: &[usize] = match stage {
                Stage::Atlas(layers) => layers,
                Stage::Solo(layer) => std::slice::from_ref(layer),
                Stage::Backdrop(command_index) => {
                    self.encode_backdrop(encoder, *command_index, backdrop_scratch);
                    continue;
                }
            };
            self.encode_layers(encoder, layers, command_layers, child_layers);
        }
    }

    /// A backdrop blurring again: what is beneath it, drawn into the scratch
    /// target, copied out and blurred.
    fn encode_backdrop(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        command_index: usize,
        backdrop_scratch: Option<&(wgpu::Texture, wgpu::TextureView)>,
    ) {
        let Some(draw) = &self.backdrop_draws[command_index] else {
            return;
        };
        let (Some(ops), Some((scratch, scratch_view)), Some(entry)) = (
            &draw.refresh,
            backdrop_scratch,
            self.backend.backdrops.entries.get(&draw.node),
        ) else {
            return;
        };
        let region = entry.region();
        // What is beneath, drawn again from scratch: the
        // surface's own target still has last frame's glass in it.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("morf backdrop beneath"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scratch_view,
                    depth_slice: None,
                    resolve_target: None,
                    // Only the region is cleared and read: ten
                    // panels are not ten clears of the screen.
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_scissor_rect(region.x, region.y, region.width, region.height);
            pass.set_pipeline(&self.backend.clear_pipeline);
            pass.set_bind_group(0, &self.backend.viewport_bind_group, &[]);
            pass.draw(0..3, 0..1);
            for op in ops {
                match *op {
                    super::super::backdrops::BeneathOp::Command(index) => {
                        self.command(&mut pass, index, region, self.surface_frame(), false)
                    }
                    super::super::backdrops::BeneathOp::Layer(layer) => {
                        self.layer(&mut pass, layer, region, self.surface_frame())
                    }
                }
            }
        }
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: scratch,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: region.x,
                    y: region.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &entry.source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: region.width,
                height: region.height,
                depth_or_array_layers: 1,
            },
        );
        for (blur_pass, level) in &entry.passes {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("morf backdrop blur"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: entry.level_view(*level),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.backend.blur_pipeline);
            pass.set_bind_group(0, &blur_pass.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// Renders the layers of one stage into their textures, and blurs them.
    fn encode_layers(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        layers: &[usize],
        command_layers: &[Option<usize>],
        child_layers: &ChildLayers,
    ) {
        // One pass per texture. An atlas stage is normally one texture;
        // one too large for a single texture spilled into more.
        let mut remaining: Vec<usize> = layers
            .iter()
            .copied()
            .filter(|layer| self.layer_targets[*layer].is_some())
            .collect();
        while let Some(&first) = remaining.first() {
            let texture = self.layer_targets[first]
                .as_ref()
                .expect("filtered above")
                .texture
                .clone();
            let (batch, rest): (Vec<usize>, Vec<usize>) =
                remaining.iter().copied().partition(|layer| {
                    self.layer_targets[*layer]
                        .as_ref()
                        .is_some_and(|target| target.texture == texture)
                });
            remaining = rest;
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("morf subtree layers"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.layer_targets[first]
                            .as_ref()
                            .expect("filtered above")
                            .view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                for &layer_index in &batch {
                    let layer = &self.list.layers[layer_index];
                    let target = self.layer_targets[layer_index]
                        .as_ref()
                        .expect("filtered above");
                    // The target holds `region` of the surface at `origin`:
                    // the viewport shifts the surface's coordinates onto
                    // it, so every instance is drawn exactly as it would be
                    // into the surface.
                    let (region, origin) = (target.region, target.origin);
                    let frame = (region, origin);
                    pass.set_viewport(
                        origin.0 as f32 - region.x as f32,
                        origin.1 as f32 - region.y as f32,
                        self.backend.width as f32,
                        self.backend.height as f32,
                        0.0,
                        1.0,
                    );
                    for &read in &target.reads {
                        let mut command_index = layer.commands.start;
                        while command_index < layer.commands.end {
                            if let Some(child) = child_layers
                                .get(&(Some(layer_index), command_index))
                                .copied()
                            {
                                self.layer(&mut pass, child, read, frame);
                                command_index =
                                    self.list.layers[child].commands.end.max(command_index + 1);
                            } else {
                                if command_layers[command_index] == Some(layer_index) {
                                    self.command(&mut pass, command_index, read, frame, true);
                                }
                                command_index += 1;
                            }
                        }
                    }
                }
            }
            for &layer_index in &batch {
                let target = self.layer_targets[layer_index]
                    .as_ref()
                    .expect("filtered above");
                for blur in [target.blur.as_ref(), target.shadow.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    for (pass_index, blur_pass) in blur.passes.iter().enumerate() {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("morf dual-kawase pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &blur.views[pass_index],
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            ..Default::default()
                        });
                        pass.set_pipeline(&self.backend.blur_pipeline);
                        pass.set_bind_group(0, &blur_pass.bind_group, &[]);
                        pass.draw(0..3, 0..1);
                    }
                }
            }
        }
    }

    /// Draws the frame into the surface's target, wherever there is damage.
    pub(super) fn encode_surface(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        damage: &[DamageRect],
        command_layers: &[Option<usize>],
        child_layers: &ChildLayers,
    ) {
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("morf frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.backend.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            for damage in damage {
                let Some((x, y, width, height)) =
                    clamp_scissor(*damage, self.backend.width, self.backend.height)
                else {
                    continue;
                };
                pass.set_scissor_rect(x, y, width, height);
                pass.set_pipeline(&self.backend.clear_pipeline);
                pass.set_bind_group(0, &self.backend.viewport_bind_group, &[]);
                pass.draw(0..3, 0..1);
                let mut command_index = 0;
                while command_index < self.list.commands.len() {
                    if let Some(layer) = child_layers.get(&(None, command_index)).copied() {
                        self.layer(&mut pass, layer, *damage, self.surface_frame());
                        command_index = self.list.layers[layer].commands.end.max(command_index + 1);
                    } else {
                        if command_layers[command_index].is_none() {
                            self.command(
                                &mut pass,
                                command_index,
                                *damage,
                                self.surface_frame(),
                                true,
                            );
                        }
                        command_index += 1;
                    }
                }
            }
        }
    }
}
