//! Registering a configuration's shaders: pipelines, textures and data blocks.
//!
//! All of it happens when the configuration loads and none of it during a
//! frame. Building a pipeline costs tens of milliseconds and decoding an image
//! costs more, and a compositor has neither to spend at paint time.

use super::backend_types::*;
use super::field_pass::*;
use super::pipelines::build_glyph_pipeline;

impl WgpuBackend {
    pub fn register_shader(&mut self, shader: ShaderRegistration<'_>) -> Result<(), GpuError> {
        let registry = if shader.effect {
            &self.effect_shaders
        } else {
            &self.shaders
        };
        if registry.contains_key(&shader.program) {
            return Ok(());
        }
        // A shader's own textures and data blocks, bound once here rather than
        // per frame: an image is decoded and uploaded when the configuration
        // says so, not while a frame is being drawn.
        let textures = self.build_shader_textures(shader.textures)?;
        let data = self.build_shader_data_layout(shader.data);
        // What naga or the driver rejects is caught here and returned, so the
        // configuration hears why instead of a log line and a dead pipeline.
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = if shader.effect {
            build_glyph_pipeline(
                &self.device,
                &self.glyph_layout,
                Some(&self.field_shader_layout),
                shader.wgsl,
                textures.as_ref().map(|(_, layout)| layout),
                data.as_ref(),
                self.space,
            )
            .ok_or_else(|| {
                GpuError::Pipeline(
                    "the glyph shader has no hook to splice an effect into".to_owned(),
                )
            })?
        } else {
            build_field_pipeline(
                &self.device,
                FieldPipeline {
                    variant: FieldVariant::General,
                    layout: &self.field_layout,
                    shader_layout: &self.field_shader_layout,
                    user: shader.wgsl,
                    owns_coverage: shader.owns_coverage,
                    vertex: shader.vertex,
                    textures: textures.as_ref().map(|(_, layout)| layout),
                    data: data.as_ref(),
                    space: self.space,
                },
            )
            .ok_or_else(|| {
                GpuError::Pipeline(
                    "the field shader has no hook to splice a shader into".to_owned(),
                )
            })?
        };
        if let Some(error) = super::backend_init::ready(scope.pop()) {
            return Err(GpuError::Pipeline(format!(
                "the GPU rejected the shader: {error}"
            )));
        }
        let registry = if shader.effect {
            &mut self.effect_shaders
        } else {
            &mut self.shaders
        };
        registry.insert(
            shader.program,
            ShaderProgram {
                pipeline,
                offsets: shader.offsets.to_vec(),
                size: shader.uniform_size,
                textures: textures.map(|(group, _)| group),
                data: data.map(|layout| (layout, shader.data.to_vec())),
            },
        );
        if !self
            .registered
            .iter()
            .any(|kept| kept.program == shader.program && kept.effect == shader.effect)
        {
            self.registered.push(KeptShader::of(&shader));
        }
        Ok(())
    }

    /// Builds every registered shader's pipeline again, for a new space.
    pub(crate) fn rebuild_shaders(&mut self) {
        self.shaders.clear();
        self.effect_shaders.clear();
        self.shader_instances.clear();
        for kept in std::mem::take(&mut self.registered) {
            if let Err(error) = self.register_shader(kept.registration()) {
                eprintln!("morf: gpu: shader {:#x} not rebuilt: {error}", kept.program);
            }
        }
    }

    /// Decodes and uploads a shader's declared textures.
    ///
    /// `None` when it declared none, which is the common case and costs nothing
    /// — an empty bind group would still be a group to create and bind.
    fn build_shader_textures(
        &mut self,
        paths: &[String],
    ) -> Result<Option<(wgpu::BindGroup, wgpu::BindGroupLayout)>, GpuError> {
        if paths.is_empty() {
            return Ok(None);
        }
        let mut entries = Vec::with_capacity(paths.len() * 2);
        for slot in 0..paths.len() as u32 {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: slot * 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: slot * 2 + 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
        }
        let layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("morf shader textures"),
                entries: &entries,
            });
        let mut views = Vec::with_capacity(paths.len());
        for path in paths {
            let image = self
                .images
                .load(path, 0, 0, 120)
                .map_err(|error| GpuError::Other(format!("shader texture `{path}`: {error}")))?;
            views.push(self.upload_shader_texture(&image));
        }
        let mut bindings = Vec::with_capacity(views.len() * 2);
        for (slot, view) in views.iter().enumerate() {
            bindings.push(wgpu::BindGroupEntry {
                binding: slot as u32 * 2,
                resource: wgpu::BindingResource::TextureView(view),
            });
            bindings.push(wgpu::BindGroupEntry {
                binding: slot as u32 * 2 + 1,
                resource: wgpu::BindingResource::Sampler(&self.glyph_sampler),
            });
        }
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("morf shader textures"),
            layout: &layout,
            entries: &bindings,
        });
        Ok(Some((group, layout)))
    }

    /// The group layout a shader's data blocks are bound through.
    ///
    /// `None` when it declared none. The buffers themselves are each node's
    /// own, made when a node first draws with it (`create_shader_instance`).
    fn build_shader_data_layout(&self, blocks: &[(String, u32)]) -> Option<wgpu::BindGroupLayout> {
        if blocks.is_empty() {
            return None;
        }
        let entries: Vec<_> = (0..blocks.len() as u32)
            .map(|slot| wgpu::BindGroupLayoutEntry {
                binding: slot,
                visibility: wgpu::ShaderStages::FRAGMENT,
                // Read-only on purpose: every pixel of a node runs this shader,
                // so a writable block would be a race between all of them.
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        Some(
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("morf shader data"),
                    entries: &entries,
                }),
        )
    }
}

/// One node's buffers for a program: its uniform block, and a storage buffer
/// per data block, with the groups that bind them.
///
/// Made the first frame a node draws with the program and kept while it does,
/// so a node wearing a shader costs what the program alone used to: one block,
/// one group, and its data buffers.
pub(crate) fn create_shader_instance(
    device: &wgpu::Device,
    shader_layout: &wgpu::BindGroupLayout,
    program: &ShaderProgram,
    frame: u64,
) -> ShaderInstance {
    let uniforms = create_shader_uniform_buffer(device, program.size);
    let bind_group = create_shader_bind_group(device, shader_layout, &uniforms);
    let data = program.data.as_ref().map(|(layout, blocks)| {
        let buffers: Vec<_> = blocks
            .iter()
            .map(|(name, length)| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(name),
                    size: u64::from(*length).max(1) * 4,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        let entries: Vec<_> = buffers
            .iter()
            .enumerate()
            .map(|(slot, buffer)| wgpu::BindGroupEntry {
                binding: slot as u32,
                resource: buffer.as_entire_binding(),
            })
            .collect();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("morf shader data"),
            layout,
            entries: &entries,
        });
        (buffers, group)
    });
    ShaderInstance {
        uniforms,
        bind_group,
        data,
        frame,
    }
}

/// A registration as owned data, so it can be made again.
pub(crate) struct KeptShader {
    program: u64,
    wgsl: Option<String>,
    vertex: Option<String>,
    offsets: Vec<u32>,
    uniform_size: u32,
    owns_coverage: bool,
    effect: bool,
    textures: Vec<String>,
    data: Vec<(String, u32)>,
}

impl KeptShader {
    fn of(shader: &ShaderRegistration<'_>) -> Self {
        Self {
            program: shader.program,
            wgsl: shader.wgsl.map(str::to_owned),
            vertex: shader.vertex.map(str::to_owned),
            offsets: shader.offsets.to_vec(),
            uniform_size: shader.uniform_size,
            owns_coverage: shader.owns_coverage,
            effect: shader.effect,
            textures: shader.textures.to_vec(),
            data: shader.data.to_vec(),
        }
    }

    fn registration(&self) -> ShaderRegistration<'_> {
        ShaderRegistration {
            program: self.program,
            wgsl: self.wgsl.as_deref(),
            vertex: self.vertex.as_deref(),
            offsets: &self.offsets,
            uniform_size: self.uniform_size,
            owns_coverage: self.owns_coverage,
            effect: self.effect,
            textures: &self.textures,
            data: &self.data,
        }
    }
}
