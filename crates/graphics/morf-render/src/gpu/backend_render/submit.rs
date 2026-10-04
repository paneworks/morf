//! Writing a frame's instances to the GPU, and submitting and presenting the
//! encoded frame.

use crate::{DamageRect, DrawList, SdfFieldInstance, SdfFieldLayer, SdfFieldMaterial};

use super::super::{backend_types::*, glyphs::GlyphBatch, targets::*, textures::*};

/// The field instances of a frame, with the layers, materials and outline
/// points they read.
pub(super) type FieldData<'a> = (
    &'a [SdfFieldInstance],
    &'a [SdfFieldLayer],
    &'a [SdfFieldMaterial],
    &'a [[f32; 2]],
);

impl WgpuBackend {
    /// Grows the instance buffers to the frame, and writes its instances and
    /// shader uniforms into them.
    pub(super) fn upload_frame(
        &mut self,
        list: &DrawList,
        scale_120: u32,
        (field_instances, field_layers, field_materials, field_outlines): FieldData<'_>,
        glyph_batch: Option<&GlyphBatch>,
        texture_batch: &TextureBatch,
    ) {
        self.ensure_textures(texture_batch.instances.len().max(1));
        self.ensure_glyphs(
            glyph_batch
                .as_ref()
                .map_or(1, |batch| batch.instances.len().max(1)),
        );
        self.ensure_fields(
            field_instances.len().max(1),
            field_layers.len().max(1),
            field_materials.len().max(1),
            field_outlines.len().max(1),
        );
        if !field_instances.is_empty() {
            self.queue
                .write_buffer(&self.field_buffer, 0, bytemuck::cast_slice(field_instances));
            self.queue.write_buffer(
                &self.field_layer_buffer,
                0,
                bytemuck::cast_slice(field_layers),
            );
            self.queue.write_buffer(
                &self.field_material_buffer,
                0,
                bytemuck::cast_slice(field_materials),
            );
            if !field_outlines.is_empty() {
                self.queue.write_buffer(
                    &self.field_outline_buffer,
                    0,
                    bytemuck::cast_slice(field_outlines),
                );
            }
        }
        self.write_shader_uniforms(list, scale_120);
        if let Some(batch) = glyph_batch {
            self.queue.write_buffer(
                &self.glyph_buffer,
                0,
                bytemuck::cast_slice(&batch.instances),
            );
        }
        if !texture_batch.instances.is_empty() {
            self.queue.write_buffer(
                &self.texture_buffer,
                0,
                bytemuck::cast_slice(&texture_batch.instances),
            );
        }
    }

    /// Composites the frame onto the surface, submits it, and presents it.
    pub(super) fn finish_frame(
        &mut self,
        mut encoder: wgpu::CommandEncoder,
        list: &DrawList,
        damage: &[DamageRect],
        reach: &[Option<DamageRect>],
        command_layers: &[Option<usize>],
        scale_120: u32,
    ) -> Result<(), GpuError> {
        // Buffers of the engine's own: only what the next one is missing is
        // copied into it, and it goes out with the frame's damage.
        if let Some(buffers) = &mut self.buffers {
            self.skipped = !buffers.encode(&self.device, &mut encoder, damage);
        }
        let frame = if let Some(surface) = &mut self.surface {
            // `None` means this frame is skipped: there is no image to draw
            // into. Everything already encoded is still submitted below —
            // offscreen layers, glyph atlases, the field pass — because that
            // work is what the next frame composites, and throwing it away
            // would make a skipped frame cost more than a drawn one.
            let Some(frame) = acquire_frame(&self.device, surface)? else {
                self.skipped = true;
                super::super::present::submit(&self.queue, Some(encoder.finish()), || {});
                return Ok(());
            };
            let frame_view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("morf surface composite"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&surface.pipeline);
                pass.set_bind_group(0, &surface.bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            Some(frame)
        } else {
            None
        };
        if let Some(profile) = &self.profile {
            profile.mark(&mut encoder, 3);
            profile.resolve(&mut encoder);
        }
        let queue = &self.queue;
        let buffers = &mut self.buffers;
        super::super::present::submit(queue, Some(encoder.finish()), || {
            if let Some(buffers) = buffers {
                buffers.before_submit(queue);
            }
        });
        // `MORF_GPU_WAIT=1` waits for the GPU here and prints what it took:
        // the one way to see a frame's cost on the GPU rather than the CPU.
        static GPU_WAIT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *GPU_WAIT.get_or_init(|| std::env::var_os("MORF_GPU_WAIT").is_some()) {
            let started = std::time::Instant::now();
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            eprintln!(
                "gpu done in {:.2} ms",
                started.elapsed().as_secs_f64() * 1e3
            );
        }
        if let Some(frame) = frame {
            self.queue.present(frame);
        }
        if let Some(buffers) = &mut self.buffers {
            buffers.present(&self.queue, damage);
        }
        if let Some(profile) = &self.profile {
            let shading = super::super::profile::Shading::of(
                list,
                damage,
                reach,
                |command| command_layers[command].is_some(),
                scale_120,
            );
            profile.report(&self.device, &shading);
        }
        Ok(())
    }
}
