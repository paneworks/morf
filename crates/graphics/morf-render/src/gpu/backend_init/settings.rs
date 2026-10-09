//! The backend's settings and accessors, and the instance buffers that grow
//! with a frame's needs.

use crate::SdfFieldInstance;

use super::super::{
    backend_types::*, clear_pipeline::*, field_pass::*, glyphs::*, pipelines::*, targets::*,
};

impl WgpuBackend {
    /// Whether this backend commits its surface itself, declaring each
    /// frame's damage with the buffer that carries it. A host then declares
    /// none of its own: a frame that finds no buffer free is committed with
    /// nothing, so the compositor draws nothing for it, and its damage goes
    /// with the next buffer.
    pub fn declares_damage(&self) -> bool {
        self.buffers.is_some()
    }

    /// Returns the selected hardware and backend identifiers.
    pub fn info(&self) -> &GpuInfo {
        &self.info
    }

    /// Waits until the GPU has finished everything submitted to it.
    ///
    /// For measuring a frame's cost on the GPU rather than the CPU's time to
    /// record it; a shell never needs to.
    pub fn wait_idle(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }

    /// Recreates the physical target and updates shader viewport dimensions.
    pub(crate) fn resize_target(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        (self.texture, self.view) = create_target(
            &self.device,
            self.width,
            self.height,
            super::super::target_format(self.blend),
        );
        // The pooled layer targets are surface-sized, so a resize retires them.
        self.layer_pool.clear();
        // So is the backdrops' scratch, and every region they were cut from.
        self.backdrops.entries.clear();
        self.backdrops.scratch = None;
        let viewport = [self.width as f32, self.height as f32, self.elapsed, 0.0];
        self.queue
            .write_buffer(&self.viewport_buffer, 0, bytemuck::cast_slice(&viewport));
        if let Some(buffers) = &mut self.buffers {
            buffers.retarget(&self.device, &self.texture);
        }
        if let Some(surface) = &mut self.surface {
            surface.config.width = self.width;
            surface.config.height = self.height;
            surface.surface.configure(&self.device, &surface.config);
            surface.bind_group = create_composite_bind_group(
                &self.device,
                &surface.texture_layout,
                &composite_view(&self.texture),
                &surface.sampler,
            );
        }
    }

    /// The space this surface blends translucent colours in.
    pub fn blend(&self) -> crate::BlendSpace {
        self.blend
    }

    /// Changes the space this surface blends in.
    ///
    /// Every target and built-in pipeline is rebuilt for it, and registered
    /// shaders are dropped: their pipelines were built for the old target, so
    /// the host registers them again. Returns whether anything changed — only
    /// then do the shaders need registering. The next frame is drawn in full,
    /// since the old target is gone.
    pub fn set_blend(&mut self, blend: crate::BlendSpace) -> bool {
        if blend == self.blend {
            return false;
        }
        self.blend = blend;
        let format = super::super::target_format(blend);
        self.clear_pipeline =
            create_clear_pipeline(&self.device, &self.clear_layout, &self.clear_shader, format);
        self.glyph_pipeline = build_glyph_pipeline(
            &self.device,
            &self.glyph_layout,
            None,
            None,
            None,
            None,
            blend,
        )
        .expect("the glyph shader carries its own hook");
        self.lcd_pipeline = self
            .subpixel
            .map(|text| build_lcd_pipeline(&self.device, &self.glyph_layout, blend, text));
        self.mask_pipeline = None;
        self.blur_pipeline = build_blur_pipeline(&self.device, &self.blur_layout, blend);
        self.field_pipeline = build_field_pipeline(
            &self.device,
            FieldPipeline {
                variant: FieldVariant::General,
                layout: &self.field_layout,
                shader_layout: &self.field_shader_layout,
                user: None,
                owns_coverage: false,
                vertex: None,
                textures: None,
                data: None,
                blend,
            },
        )
        .expect("the field shader carries its own hook");
        let specialised = |variant| {
            build_field_pipeline(
                &self.device,
                FieldPipeline {
                    layout: &self.field_layout,
                    shader_layout: &self.field_shader_layout,
                    variant,
                    user: None,
                    owns_coverage: false,
                    vertex: None,
                    textures: None,
                    data: None,
                    blend,
                },
            )
            .expect("the field shader carries its own hook")
        };
        self.field_analytic = specialised(FieldVariant::Analytic);
        self.field_opaque = specialised(FieldVariant::AnalyticOpaque);
        self.field_boxes = specialised(FieldVariant::Boxes);
        self.field_boxes_opaque = specialised(FieldVariant::BoxesOpaque);
        self.field_quad = specialised(FieldVariant::Quad);
        self.field_boxes_uniform = specialised(FieldVariant::BoxesUniform);
        self.shaders.clear();
        self.effect_shaders.clear();
        self.shader_instances.clear();
        self.resize_target(self.width, self.height);
        true
    }

    /// Whether this device can draw subpixel text at all.
    pub fn supports_subpixel_text(&self) -> bool {
        self.lcd_supported
    }

    /// Draws text in subpixels, where it is safe to (see `lcd.rs`), or not.
    ///
    /// Returns whether anything changed: the pipeline is built for the new
    /// setting, and the next frame has to be drawn in full, since every
    /// glyph already on the surface was drawn the old way. On a device
    /// without dual-source blending it stays off.
    pub fn set_subpixel_text(&mut self, text: Option<crate::SubpixelText>) -> bool {
        let text = text.filter(|_| self.lcd_supported);
        if text == self.subpixel {
            return false;
        }
        self.subpixel = text;
        self.lcd_pipeline =
            text.map(|text| build_lcd_pipeline(&self.device, &self.glyph_layout, self.blend, text));
        true
    }

    /// The subpixel text setting in force.
    pub fn subpixel_text(&self) -> Option<crate::SubpixelText> {
        self.subpixel
    }

    /// Whether the whole surface is declared opaque to the compositor, which
    /// makes all of it ground subpixel text may be drawn on. Returns whether
    /// it changed.
    pub fn set_opaque_surface(&mut self, opaque: bool) -> bool {
        let changed = self.opaque_surface != opaque;
        self.opaque_surface = opaque;
        changed
    }

    /// Returns the persistent target for copying or diagnostics.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// The shaper this renderer draws text with.
    ///
    /// Lent out so a text input's caret can be read off the very buffer its
    /// glyphs are drawn from, rather than a second shaping of the same text
    /// that could disagree with it by a subpixel.
    pub fn text_system(&mut self) -> &mut morf_text::TextSystem {
        &mut self.text
    }

    /// The images this backend draws from, for reading what became of a
    /// source: its size, whether it moves, why it failed.
    pub fn image_cache(&mut self) -> &mut morf_image::ImageCache {
        &mut self.images
    }

    /// Registers a compiled shader, building its pipeline.
    ///
    /// Called when a configuration loads, never while rendering: compiling a
    /// pipeline costs tens of milliseconds, and a compositor cannot spend that
    /// at paint time. Registering the same program twice is a no-op, so a
    /// configuration that attaches one shader to fifty nodes builds one
    /// pipeline.
    /// Advances the clock shaders read.
    ///
    /// Called once per frame by the host, which owns the frame clock; the
    /// backend only needs the number a shader will see.
    pub fn set_elapsed(&mut self, seconds: f32) {
        self.elapsed = seconds;
    }

    /// Whether a program has been registered, in either registry.
    pub fn has_shader(&self, program: u64) -> bool {
        self.shaders.contains_key(&program) || self.effect_shaders.contains_key(&program)
    }

    /// Grows the field instance, layer, material and outline buffers, rebinding
    /// whenever one of the storage buffers moves.
    pub(crate) fn ensure_fields(
        &mut self,
        instances: usize,
        layers: usize,
        materials: usize,
        outlines: usize,
    ) {
        if instances > self.field_capacity {
            self.field_capacity = instances.next_power_of_two();
            self.field_buffer = create_instance_buffer_for::<SdfFieldInstance>(
                &self.device,
                self.field_capacity,
                "morf field instances",
            );
        }
        let mut rebind = false;
        if layers > self.field_layer_capacity {
            self.field_layer_capacity = layers.next_power_of_two();
            self.field_layer_buffer =
                create_field_layer_buffer(&self.device, self.field_layer_capacity);
            rebind = true;
        }
        if materials > self.field_material_capacity {
            self.field_material_capacity = materials.next_power_of_two();
            self.field_material_buffer =
                create_field_material_buffer(&self.device, self.field_material_capacity);
            rebind = true;
        }
        if outlines > self.field_outline_capacity {
            self.field_outline_capacity = outlines.next_power_of_two();
            self.field_outline_buffer =
                create_field_outline_buffer(&self.device, self.field_outline_capacity);
            rebind = true;
        }
        if rebind {
            // The bind group holds the old buffers, so it has to be rebuilt
            // whenever either storage grows or the shader reads freed memory.
            self.field_bind_group = create_field_bind_group(
                &self.device,
                &self.field_layout,
                &self.viewport_buffer,
                &self.field_layer_buffer,
                &self.field_material_buffer,
                &self.field_outline_buffer,
            );
        }
    }

    pub(crate) fn ensure_glyphs(&mut self, required: usize) {
        if required <= self.glyph_capacity {
            return;
        }
        self.glyph_capacity = required.next_power_of_two();
        self.glyph_buffer = create_glyph_buffer(&self.device, self.glyph_capacity);
    }

    pub(crate) fn ensure_textures(&mut self, required: usize) {
        if required <= self.texture_capacity {
            return;
        }
        self.texture_capacity = required.next_power_of_two();
        self.texture_buffer = create_instance_buffer_for::<GlyphInstance>(
            &self.device,
            self.texture_capacity,
            "morf texture instances",
        );
    }
}
