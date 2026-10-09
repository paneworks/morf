//! Drawing one command or one layer into a pass, as every pass of a frame does,
//! and where each command can put pixels.

use crate::effects::physical_damage;
use crate::{DamageRect, DrawList, ShaderBinding};

use super::super::{
    backdrops::BackdropDraw,
    backend_types::*,
    glyphs::{GlyphBatch, GlyphInstance},
    targets::*,
    textures::*,
};

/// What a frame draws from: the backend, the list, and the batches built for
/// it. Every pass draws its commands and layers through this.
pub(super) struct FrameDraw<'a> {
    pub(super) backend: &'a WgpuBackend,
    pub(super) list: &'a DrawList,
    pub(super) scale_120: u32,
    pub(super) reach: &'a [Option<DamageRect>],
    pub(super) field_indices: &'a [Option<std::ops::Range<u32>>],
    pub(super) field_shaders: &'a [Option<ShaderBinding>],
    pub(super) texture_batch: &'a TextureBatch,
    pub(super) backdrop_draws: &'a [Option<BackdropDraw>],
    pub(super) glyph_batch: Option<&'a GlyphBatch>,
    pub(super) layer_targets: &'a [Option<LayerTarget>],
}

impl FrameDraw<'_> {
    /// Draws command `command_index` where `base_damage` reaches it, into a
    /// target holding `frame`; `lcd` lets its glyphs be drawn in subpixels.
    pub(super) fn command(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        command_index: usize,
        base_damage: DamageRect,
        frame: (DamageRect, (u32, u32)),
        lcd: bool,
    ) {
        let command_damage = if let Some(clip) = self.list.commands[command_index].clip() {
            physical_damage(clip, self.scale_120)
                .and_then(|clip| intersect_damage(base_damage, clip))
        } else {
            Some(base_damage)
        }
        .filter(|damage| {
            self.reach[command_index]
                .is_some_and(|reach| intersect_damage(*damage, reach).is_some())
        });
        if let Some(command_damage) = command_damage
            && let Some((x, y, width, height)) = placed_scissor(command_damage, frame)
        {
            pass.set_scissor_rect(x, y, width, height);
            let measured = self
                .backend
                .profile
                .as_ref()
                .and_then(|profile| profile.begin_draw(pass, command_index));
            if let Some(instances) = self.field_indices[command_index].clone() {
                // A shader replaces the pipeline rather than switching
                // inside it: WGSL cannot swap a function at run time,
                // and a uniform branch would make every node without a
                // shader pay for the ones that have one.
                // The pipeline is the program's; the block and the
                // data it reads are this node's own.
                let program = self.backend.shader_instance(
                    self.list.commands[command_index].node(),
                    self.field_shaders[instances.start as usize].as_ref(),
                    false,
                );
                match program {
                    Some((program, instance)) => {
                        pass.set_pipeline(&program.pipeline);
                        pass.set_bind_group(1, &instance.bind_group, &[]);
                        // Groups two and three exist only when the
                        // shader declared textures or data blocks, and
                        // the pipeline layout matches — so binding them
                        // is conditional on the same thing the layout
                        // was built from.
                        if let Some(textures) = &program.textures {
                            pass.set_bind_group(2, textures, &[]);
                        }
                        if let Some((_, data)) = &instance.data {
                            pass.set_bind_group(3, data, &[]);
                        }
                    }
                    None => {
                        use super::super::field_pass::FieldVariant;
                        pass.set_pipeline(
                            match FieldVariant::for_command(&self.list.commands[command_index]) {
                                FieldVariant::General => &self.backend.field_pipeline,
                                FieldVariant::Analytic => &self.backend.field_analytic,
                                FieldVariant::AnalyticOpaque => &self.backend.field_opaque,
                                FieldVariant::Boxes => &self.backend.field_boxes,
                                FieldVariant::BoxesOpaque => &self.backend.field_boxes_opaque,
                                FieldVariant::Quad => &self.backend.field_quad,
                                FieldVariant::BoxesUniform => &self.backend.field_boxes_uniform,
                            },
                        );
                        pass.set_bind_group(1, &self.backend.field_shader_default, &[]);
                    }
                }
                pass.set_bind_group(0, &self.backend.field_bind_group, &[]);
                pass.set_vertex_buffer(0, self.backend.field_buffer.slice(..));
                // Four vertices as a strip: the shader expands the quad
                // by the outline and the softened edge itself.
                pass.draw(0..4, instances);
            }
            if let Some(instance) = self.texture_batch.command_instances[command_index] {
                let image = &self.texture_batch.images[instance as usize];
                pass.set_pipeline(&self.backend.glyph_pipeline);
                pass.set_bind_group(0, &image.bind_group, &[]);
                pass.set_vertex_buffer(0, self.backend.texture_buffer.slice(..));
                pass.draw(0..6, instance..instance + 1);
            }
            if let Some(draw) = &self.backdrop_draws[command_index]
                && let Some(entry) = self.backend.backdrops.entries.get(&draw.node)
            {
                pass.set_pipeline(&self.backend.glyph_pipeline);
                pass.set_bind_group(0, &entry.bind_group, &[]);
                pass.set_vertex_buffer(0, self.backend.texture_buffer.slice(..));
                pass.draw(0..6, draw.instance..draw.instance + 1);
            }
            if let Some(batch) = self.glyph_batch {
                for span in &batch.command_spans[command_index] {
                    // Subpixel only into the target the glyph was
                    // judged for (its layer, or the surface): the same
                    // command drawn again beneath a backdrop goes to
                    // a scratch texture cleared transparent.
                    match (&self.backend.lcd_pipeline, span.lcd && lcd) {
                        (Some(lcd), true) => pass.set_pipeline(lcd),
                        _ => pass.set_pipeline(&self.backend.glyph_pipeline),
                    }
                    let atlas = if span.color {
                        &self.backend.glyph_color_atlas
                    } else {
                        &self.backend.glyph_mask_atlas
                    };
                    pass.set_bind_group(0, &atlas.bind_group, &[]);
                    pass.set_vertex_buffer(0, self.backend.glyph_buffer.slice(..));
                    pass.draw(0..6, span.range.clone());
                }
            }
            if let Some(end) = measured {
                self.backend.profile.as_ref().unwrap().end_draw(pass, end);
            }
        }
    }

    /// Composites layer `layer_index` where `base_damage` reaches it, into a
    /// target holding `frame`.
    pub(super) fn layer(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        layer_index: usize,
        base_damage: DamageRect,
        frame: (DamageRect, (u32, u32)),
    ) {
        // A mask is read by the layer it masks, never drawn itself.
        // A layer nothing reads this frame was not rendered, and the
        // damage here does not reach it either.
        if self.list.layers[layer_index].mask_for.is_none()
            && let Some(target) = &self.layer_targets[layer_index]
            && let Some(layer_damage) =
                physical_damage(self.list.layers[layer_index].bounds, self.scale_120)
                    .and_then(|bounds| intersect_damage(base_damage, bounds))
            && let Some((x, y, width, height)) = placed_scissor(layer_damage, frame)
        {
            pass.set_scissor_rect(x, y, width, height);
            // An effect shader composites the layer instead of the
            // plain texture pass: by now the subtree is a texture, so
            // there is finally something for it to sample.
            let effect = self.backend.shader_instance(
                self.list.layers[layer_index].node,
                self.list.layers[layer_index].shader.as_ref(),
                true,
            );
            match effect {
                Some((program, instance)) => {
                    pass.set_pipeline(&program.pipeline);
                    pass.set_bind_group(1, &instance.bind_group, &[]);
                    // As in the field pass: groups two and three exist
                    // only when the shader declared textures or data
                    // blocks, and the layout was built from the same
                    // condition.
                    if let Some(textures) = &program.textures {
                        pass.set_bind_group(2, textures, &[]);
                    }
                    if let Some((_, data)) = &instance.data {
                        pass.set_bind_group(3, data, &[]);
                    }
                }
                None => pass.set_pipeline(&self.backend.glyph_pipeline),
            }
            if let (Some(bind_group), Some(instance)) =
                (&target.shadow_bind_group, target.shadow_instance)
            {
                pass.set_bind_group(0, bind_group, &[]);
                pass.set_vertex_buffer(0, self.backend.texture_buffer.slice(..));
                pass.draw(0..6, instance..instance + 1);
            }
            let masked = match (&target.alpha_mask, &self.backend.mask_pipeline) {
                (Some(mask), Some(pipeline)) => {
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(1, mask, &[]);
                    true
                }
                _ => false,
            };
            // A mask whose target is missing covers nothing: the
            // layer shows only where the mask is inverted.
            let hidden = !masked
                && self.list.layers[layer_index]
                    .alpha_mask
                    .is_some_and(|mask| !mask.invert);
            if !hidden {
                pass.set_bind_group(0, &target.bind_group, &[]);
                pass.set_vertex_buffer(0, self.backend.texture_buffer.slice(..));
                pass.draw(0..6, target.instance..target.instance + 1);
            }
        }
    }
}

// Where each command can put pixels: its bounds, and every quad it is
// drawn with, since a glyph may reach past the box it was laid out in.
// A damage rectangle that misses it skips its draws — a clock ticking
// in ten panels is ten small rectangles, and each used to issue every
// draw of the surface, scissored to nothing.
pub(super) fn command_reach(
    backend: &WgpuBackend,
    list: &DrawList,
    scale_120: u32,
    (field_indices, field_shaders): (&[Option<std::ops::Range<u32>>], &[Option<ShaderBinding>]),
    texture_batch: &TextureBatch,
    backdrop_draws: &[Option<BackdropDraw>],
    glyph_batch: Option<&GlyphBatch>,
) -> Vec<Option<DamageRect>> {
    let target = (backend.width, backend.height);
    let mut reach: Vec<Option<DamageRect>> = list
        .commands
        .iter()
        .enumerate()
        .map(|(index, command)| {
            // A configuration's shader may move its quad anywhere, so
            // a command wearing one is drawn wherever there is damage.
            let shaded = field_indices[index]
                .as_ref()
                .is_some_and(|instances| field_shaders[instances.start as usize].is_some());
            if shaded {
                return Some(DamageRect {
                    x: 0,
                    y: 0,
                    width: backend.width,
                    height: backend.height,
                });
            }
            // A pixel of margin for the antialiased edge.
            physical_damage(command.bounds(), scale_120).map(grow_damage)
        })
        .collect();
    let mut widen = |index: usize, instance: &GlyphInstance| {
        let quad = quad_reach(instance, target);
        reach[index] = Some(reach[index].map_or(quad, |seen| union_damage(seen, quad)));
    };
    for (index, instance) in texture_batch.command_instances.iter().enumerate() {
        if let Some(instance) = instance {
            widen(index, &texture_batch.instances[*instance as usize]);
        }
    }
    for (index, draw) in backdrop_draws.iter().enumerate() {
        if let Some(draw) = draw {
            widen(index, &texture_batch.instances[draw.instance as usize]);
        }
    }
    if let Some(batch) = glyph_batch {
        for (index, spans) in batch.command_spans.iter().enumerate() {
            for span in spans {
                for instance in &batch.instances[span.range.start as usize..span.range.end as usize]
                {
                    widen(index, instance);
                }
            }
        }
    }
    reach
}
