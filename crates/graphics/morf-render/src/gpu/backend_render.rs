use crate::{DamageRect, DrawList, RenderBackend};
use morf_layout::{Size, TextMeasurer, TextOptions};
use morf_scene::{Element, NodeHandle};
use std::collections::HashMap;

mod frame_draw;
mod split;
mod stages;
mod submit;

use super::{backend_types::*, batches::*, glyph_batch::*, textures::*};

impl TextMeasurer for WgpuBackend {
    fn measure(
        &mut self,
        node: NodeHandle,
        text: &str,
        family: &str,
        size: f64,
        options: TextOptions,
    ) -> Size {
        self.text.measure(node, text, family, size, options)
    }

    fn measure_image(
        &mut self,
        _node: NodeHandle,
        element: Element,
        source: &str,
        theme: Option<&str>,
    ) -> Option<Size> {
        if source.is_empty() {
            return None;
        }
        let (width, height) = match element {
            Element::Image => self.images.intrinsic_size(source).ok()?,
            Element::Icon => self
                .images
                .icon_intrinsic_size(source, theme.unwrap_or("hicolor"), 48)
                .ok()?,
            _ => return None,
        };
        Some(Size {
            width: f64::from(width),
            height: f64::from(height),
        })
    }
}

impl RenderBackend for WgpuBackend {
    type Error = GpuError;

    fn resize(&mut self, width: u32, height: u32) {
        self.resize_target(width, height);
    }

    fn render(
        &mut self,
        list: &DrawList,
        damage: &[DamageRect],
        scale_120: u32,
    ) -> Result<(), Self::Error> {
        let mut split = split::RenderSplit::start();
        let FieldBatch {
            indices: field_indices,
            instances: field_instances,
            layers: field_layers,
            materials: field_materials,
            outlines: field_outlines,
            shaders: field_shaders,
        } = collect_field_instances(list, scale_120, &mut self.text, &mut self.drawings);
        split.mark("fields");
        let mut glyph_batch = create_glyph_batch(
            GlyphBatchContext {
                queue: &self.queue,
                mask_atlas: &mut self.glyph_mask_atlas,
                color_atlas: &mut self.glyph_color_atlas,
                target_size: (self.width, self.height),
            },
            &mut self.text,
            list,
            scale_120,
        )?;
        split.mark("glyphs");
        let mut texture_batch = create_texture_batch(
            TextureBatchContext {
                device: &self.device,
                queue: &self.queue,
                layout: &self.glyph_layout,
                sampler: &self.glyph_sampler,
                nearest: &self.nearest_sampler,
                target_size: (self.width, self.height),
                external: &self.external_textures,
            },
            &mut self.images,
            &mut self.image_textures,
            list,
            scale_120,
        );
        split.mark("images");
        push_path_textures(
            TextureBatchContext {
                device: &self.device,
                queue: &self.queue,
                layout: &self.glyph_layout,
                sampler: &self.glyph_sampler,
                nearest: &self.nearest_sampler,
                target_size: (self.width, self.height),
                external: &self.external_textures,
            },
            &mut self.path_outlines,
            &mut self.path_textures,
            list,
            scale_120,
            &mut texture_batch,
        );
        split.mark("paths");
        let scale = scale_120.max(1) as f64 / 120.0;
        let mut command_layers = vec![None; list.commands.len()];
        let mut child_layers = HashMap::new();
        for (layer_index, layer) in list.layers.iter().enumerate() {
            for owner in &mut command_layers[layer.commands.clone()] {
                *owner = Some(layer_index);
            }
            // An empty layer owns no commands, so it must not claim the index
            // of the one that follows it: the frame loop jumps to a layer's
            // `commands.end` after drawing it, and for an empty layer that is
            // the command it was standing in front of.
            if !layer.commands.is_empty() {
                child_layers.insert((layer.parent, layer.commands.start), layer_index);
            }
        }
        // Subpixel text, where it is safe (lcd.rs): at a whole-number scale,
        // over opaque ground of the same target -- the surface, or a layer
        // whose composite keeps its pixels.
        if let Some(batch) = &mut glyph_batch
            && self.lcd_pipeline.is_some()
            && scale_120.is_multiple_of(120)
        {
            super::lcd_spans::mark_subpixel_glyphs(
                batch,
                list,
                |command| command_layers[command],
                scale_120,
                (self.width, self.height),
                self.opaque_surface,
            );
        }
        let backdrop_draws = self.prepare_backdrops(
            list,
            &mut texture_batch,
            (&command_layers, &child_layers),
            scale_120,
        );
        let backdrop_scratch = backdrop_draws
            .iter()
            .flatten()
            .any(|draw| draw.refresh.is_some())
            .then(|| self.backdrop_scratch());
        // What each layer has to hold this frame: where the damage composites
        // it, and where a backdrop blurring again composites it beneath itself.
        let backdrop_reads: Vec<(DamageRect, Vec<usize>)> = backdrop_draws
            .iter()
            .flatten()
            .filter_map(|draw| {
                let ops = draw.refresh.as_ref()?;
                let entry = self.backdrops.entries.get(&draw.node)?;
                let layers = ops
                    .iter()
                    .filter_map(|op| match *op {
                        super::backdrops::BeneathOp::Layer(layer) => Some(layer),
                        super::backdrops::BeneathOp::Command(_) => None,
                    })
                    .collect();
                Some((entry.region(), layers))
            })
            .collect();
        let regions = super::layer_pool::layer_regions(
            list,
            damage,
            &backdrop_reads,
            scale_120,
            (self.width, self.height),
        );
        let stages = super::layer_pool::schedule(
            list,
            &super::backdrops::offscreen_order(list, &command_layers, &child_layers),
            &regions
                .iter()
                .map(|region| region.as_ref().map(|region| region.bounds))
                .collect::<Vec<_>>(),
            |command| {
                backdrop_draws[command]
                    .as_ref()
                    .is_some_and(|draw| draw.refresh.is_some())
            },
        );
        split.mark("backdrops and layer schedule");
        if self.mask_pipeline.is_none()
            && list.layers.iter().any(|layer| layer.alpha_mask.is_some())
        {
            self.mask_pipeline = Some(super::pipelines::build_mask_pipeline(
                &self.device,
                &self.glyph_layout,
                self.blend,
            ));
        }
        self.layer_pool.begin_frame();
        let layer_targets =
            self.build_layer_targets(list, &mut texture_batch, scale, &regions, &stages);
        self.layer_pool.end_frame();
        split.mark("layer targets");
        self.upload_frame(
            list,
            scale_120,
            (
                &field_instances,
                &field_layers,
                &field_materials,
                &field_outlines,
            ),
            glyph_batch.as_ref(),
            &texture_batch,
        );
        // Where each command can put pixels: its bounds, and every quad it is
        // drawn with, since a glyph may reach past the box it was laid out in.
        // A damage rectangle that misses it skips its draws — a clock ticking
        // in ten panels is ten small rectangles, and each used to issue every
        // draw of the surface, scissored to nothing.
        split.mark("upload");
        let reach = frame_draw::command_reach(
            self,
            list,
            scale_120,
            (&field_indices, &field_shaders),
            &texture_batch,
            &backdrop_draws,
            glyph_batch.as_ref(),
        );
        split.mark("reach");
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("morf frame encoder"),
            });
        if let Some(profile) = &self.profile {
            profile.mark(&mut encoder, 0);
        }
        let draw = frame_draw::FrameDraw {
            backend: self,
            list,
            scale_120,
            reach: &reach,
            field_indices: &field_indices,
            field_shaders: &field_shaders,
            texture_batch: &texture_batch,
            backdrop_draws: &backdrop_draws,
            glyph_batch: glyph_batch.as_ref(),
            layer_targets: &layer_targets,
        };
        draw.encode_stages(
            &mut encoder,
            &stages,
            backdrop_scratch.as_ref(),
            &command_layers,
            &child_layers,
        );
        if let Some(profile) = &self.profile {
            profile.mark(&mut encoder, 1);
        }
        draw.encode_surface(&mut encoder, damage, &command_layers, &child_layers);
        if let Some(profile) = &self.profile {
            profile.mark(&mut encoder, 2);
        }
        split.mark("encode");
        let finished = self.finish_frame(encoder, list, damage, &reach, &command_layers, scale_120);
        split.mark("acquire, submit, present");
        split.finish();
        finished
    }
}
