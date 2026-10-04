use crate::SdfFieldInstance;
use morf_image::ImageCache;
use morf_text::{RasterContent, TextSystem};
use morf_value::present::BufferSink;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::collections::HashMap;

mod device;
mod settings;

pub use device::opened_device_count;
use device::{SharedDevice, device_for, shared_instance};

/// How many buffers a surface presents through at most
/// (`MORF_PRESENT_BUFFERS` overrides it). A compositor holds the buffer it
/// shows, the one committed after it, and -- until the GPU work that read it
/// has finished -- the one before; on a busy GPU that one lingers. A fourth
/// leaves one to draw into.
fn ring_buffers() -> usize {
    std::env::var("MORF_PRESENT_BUFFERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|count| (1..=8).contains(count))
        .unwrap_or(4)
}

use wgpu::util::DeviceExt;

use super::{
    backend_types::*, clear_pipeline::*, field_pass::*, glyphs::*, pipelines::*, shaders::*,
    targets::*,
};

impl WgpuBackend {
    /// Selects a Vulkan or GLES adapter and creates an offscreen render target.
    pub async fn new(width: u32, height: u32) -> Result<Self, GpuError> {
        Self::initialize(shared_instance(), None, width, height).await
    }

    /// Creates a renderer presenting to an owned native window target.
    ///
    /// `sink` is the window's own way of showing dmabufs, when its backend
    /// has one (or why not): then the frames are presented through buffers
    /// of this engine's own (`present`), and a frame costs what it changed.
    /// Without one, or a device that cannot, the window gets a swapchain.
    /// `MORF_PRESENT=swapchain` asks for the swapchain.
    pub async fn new_surface<T>(
        window: T,
        sink: Result<Box<dyn BufferSink>, String>,
        width: u32,
        height: u32,
    ) -> Result<Self, GpuError>
    where
        T: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static,
    {
        let instance = shared_instance();
        let wanted = std::env::var("MORF_PRESENT").map_or(true, |value| value != "swapchain");
        let linked: Result<(), String> = match sink {
            Ok(sink) if wanted => {
                let mut backend = Self::initialize(instance.clone(), None, width, height).await?;
                match super::present_link::BufferLink::connect(
                    &backend.device,
                    backend.dmabuf.as_ref(),
                    sink,
                ) {
                    Ok(link) => {
                        backend.buffers = Some(super::present::BufferRing::new(
                            &backend.device,
                            &backend.texture,
                            ring_buffers(),
                            Some(link),
                        ));
                        // The sink holds the window's connection and
                        // surface: the handles are not needed again.
                        drop(window);
                        return Ok(backend);
                    }
                    Err(error) => Err(error),
                }
            }
            Ok(_) => Err("MORF_PRESENT=swapchain".to_owned()),
            Err(error) => Err(error),
        };
        if let Err(error) = linked
            && std::env::var_os("MORF_GPU_LOG").is_some()
        {
            eprintln!("morf: gpu: presenting through the swapchain: {error}");
        }
        let surface = instance
            .create_surface(window)
            .map_err(|error| GpuError(format!("could not create GPU surface: {error}")))?;
        Self::initialize(instance, Some(surface), width, height).await
    }

    async fn initialize(
        instance: wgpu::Instance,
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, GpuError> {
        let started = std::time::Instant::now();
        let opened_before = opened_device_count();
        let SharedDevice {
            adapter,
            device,
            queue,
            dmabuf,
            lcd_supported,
        } = device_for(&instance, surface.as_ref()).await?;
        let device_ready = started.elapsed();
        let adapter_info = adapter.get_info();
        let viewport_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("morf viewport layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let viewport = [width.max(1) as f32, height.max(1) as f32, 0.0, 0.0];
        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("morf viewport"),
            contents: bytemuck::cast_slice(&viewport),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("morf viewport bind group"),
            layout: &viewport_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });
        let clear_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("morf damage clear shader"),
            source: wgpu::ShaderSource::Wgsl(
                fullscreen_source(include_str!("../clear.wgsl")).into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("morf clear pipeline layout"),
            bind_group_layouts: &[Some(&viewport_layout)],
            immediate_size: 0,
        });
        let blend = crate::BlendSpace::default();
        let format = super::target_format(blend);
        let clear_pipeline =
            create_clear_pipeline(&device, &pipeline_layout, &clear_shader, format);
        let (glyph_pipeline, glyph_layout, glyph_sampler) = create_glyph_pipeline(&device, blend);
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("morf nearest sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let glyph_mask_atlas =
            GlyphAtlas::new(&device, &glyph_layout, &glyph_sampler, RasterContent::Mask);
        let glyph_color_atlas =
            GlyphAtlas::new(&device, &glyph_layout, &glyph_sampler, RasterContent::Color);
        let (blur_pipeline, blur_layout, blur_sampler) = create_blur_pipeline(&device, blend);
        let glyph_capacity = 1;
        let glyph_buffer = create_glyph_buffer(&device, glyph_capacity);
        let texture_capacity = 1;
        let texture_buffer = create_instance_buffer_for::<GlyphInstance>(
            &device,
            texture_capacity,
            "morf texture instances",
        );
        let (field_layout, field_shader_layout) = create_field_layouts(&device);
        let field_pipeline = build_field_pipeline(
            &device,
            FieldPipeline {
                layout: &field_layout,
                shader_layout: &field_shader_layout,
                user: None,
                owns_coverage: false,
                vertex: None,
                textures: None,
                data: None,
                blend,
            },
        )
        .expect("the field shader carries its own hook");
        let field_shader_default = create_shader_bind_group(
            &device,
            &field_shader_layout,
            &create_shader_uniform_buffer(&device, morf_value::shader_abi::HEADER_BYTES),
        );
        let field_capacity = 1;
        let field_buffer = create_instance_buffer_for::<SdfFieldInstance>(
            &device,
            field_capacity,
            "morf field instances",
        );
        let field_layer_capacity = 1;
        let field_layer_buffer = create_field_layer_buffer(&device, field_layer_capacity);
        let field_material_capacity = 1;
        let field_material_buffer = create_field_material_buffer(&device, field_material_capacity);
        let field_outline_capacity = 1;
        let field_outline_buffer = create_field_outline_buffer(&device, field_outline_capacity);
        let field_bind_group = create_field_bind_group(
            &device,
            &field_layout,
            &viewport_buffer,
            &field_layer_buffer,
            &field_material_buffer,
            &field_outline_buffer,
        );
        let (texture, view) = create_target(&device, width, height, format);
        let surface = surface
            .map(|surface| {
                create_surface_state(
                    &device,
                    &adapter,
                    surface,
                    &composite_view(&texture),
                    width,
                    height,
                )
            })
            .transpose()?;

        // `MORF_GPU_LOG=1`: what bringing a backend up cost, and how much of
        // it was the device.
        if std::env::var_os("MORF_GPU_LOG").is_some() {
            eprintln!(
                "morf: gpu: backend {}x{} ready in {:.1} ms; device {} in {:.1} ms",
                width,
                height,
                started.elapsed().as_secs_f64() * 1000.0,
                if opened_device_count() > opened_before {
                    "opened"
                } else {
                    "shared"
                },
                device_ready.as_secs_f64() * 1000.0
            );
        }
        let profile = super::profile::GpuProfile::new(&device, &queue);
        Ok(Self {
            device,
            queue,
            blend,
            clear_pipeline,
            clear_layout: pipeline_layout,
            clear_shader,
            viewport_buffer,
            viewport_bind_group,
            glyph_pipeline,
            mask_pipeline: None,
            lcd_supported,
            subpixel: None,
            lcd_pipeline: None,
            opaque_surface: false,
            glyph_layout,
            glyph_sampler,
            nearest_sampler,
            glyph_mask_atlas,
            glyph_color_atlas,
            warmed_text: std::collections::HashMap::new(),
            blur_pipeline,
            blur_layout,
            blur_sampler,
            glyph_buffer,
            glyph_capacity,
            texture_buffer,
            texture_capacity,
            field_pipeline,
            field_layout,
            field_buffer,
            field_capacity,
            field_layer_buffer,
            field_layer_capacity,
            field_material_buffer,
            field_material_capacity,
            field_outline_capacity,
            field_outline_buffer,
            field_bind_group,
            field_shader_layout,
            field_shader_default,
            shaders: HashMap::new(),
            effect_shaders: HashMap::new(),
            shader_instances: HashMap::new(),
            shader_frame: 0,
            elapsed: 0.0,
            images: ImageCache::default(),
            image_textures: HashMap::new(),
            path_textures: HashMap::new(),
            path_outlines: Default::default(),
            layer_pool: Default::default(),
            backdrops: Default::default(),
            text: TextSystem::new(),
            drawings: morf_vector::svg::SvgOutlines::new(),
            texture,
            view,
            surface,
            buffers: None,
            skipped: false,
            profile,
            width: width.max(1),
            height: height.max(1),
            info: GpuInfo {
                name: adapter_info.name,
                backend: adapter_info.backend,
                vendor: adapter_info.vendor,
                device: adapter_info.device,
                dmabuf: dmabuf.is_some(),
            },
            dmabuf,
            external_textures: HashMap::new(),
            pending_exports: HashMap::new(),
        })
    }
}
