use crate::SdfFieldInstance;
use morf_image::ImageCache;
use morf_text::{RasterContent, TextSystem};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

use super::{
    backend_types::*, clear_pipeline::*, field_pass::*, glyphs::*, pipelines::*, shaders::*,
    targets::*,
};

/// Opens the adapter's device with the dmabuf extensions enabled, when it can.
///
/// `None` is the ordinary case on anything that is not Vulkan, or is Vulkan
/// without the extensions: the caller opens the device the usual way and
/// captures go through shared memory. Enabling extensions is the one thing
/// that has to happen at creation, which is why this reaches into wgpu-hal at
/// all -- everything else dmabuf needs can be done on the device afterwards.
fn open_with_dmabuf(
    instance: &wgpu::Instance,
    adapter: &wgpu::Adapter,
    descriptor: &wgpu::DeviceDescriptor<'_>,
) -> Option<(wgpu::Device, wgpu::Queue, crate::gpu::dmabuf::DmabufSupport)> {
    use crate::gpu::dmabuf;
    use wgpu::hal::Instance as _;
    use wgpu::hal::api::Vulkan;
    if adapter.get_info().backend != wgpu::Backend::Vulkan {
        return None;
    }
    let hal_instance = unsafe { instance.as_hal::<Vulkan>() }?;
    let raw_instance = hal_instance.shared_instance().raw_instance();
    // The same physical device wgpu chose, found again among hal's adapters:
    // hal hands out its own adapter objects, and the one to open is the one
    // whose device is the device wgpu already reported.
    let wanted = adapter.get_info();
    let exposed = unsafe { hal_instance.enumerate_adapters(None) }
        .into_iter()
        .find(|exposed| exposed.info.device == wanted.device && exposed.info.name == wanted.name)?;
    let physical = exposed.adapter.raw_physical_device();
    let extensions = dmabuf::supported_extensions(raw_instance, physical)?;
    let render_node = dmabuf::render_node(raw_instance, physical);
    let open = unsafe {
        exposed.adapter.open_with_callback(
            descriptor.required_features,
            &descriptor.required_limits,
            &descriptor.memory_hints,
            Some(Box::new(
                |args: wgpu::hal::vulkan::CreateDeviceCallbackArgs<'_, '_, '_>| {
                    for extension in &extensions {
                        if !args.extensions.contains(extension) {
                            args.extensions.push(extension);
                        }
                    }
                },
            )),
        )
    }
    .ok()?;
    let queue_family = open.device.queue_family_index();
    let hal_adapter = unsafe { instance.create_adapter_from_hal::<Vulkan>(exposed) };
    let (device, queue) =
        unsafe { hal_adapter.create_device_from_hal::<Vulkan>(open, descriptor) }.ok()?;
    Some((
        device,
        queue,
        dmabuf::DmabufSupport {
            render_node,
            queue_family,
        },
    ))
}

/// The one GPU instance of the process, which every backend opens its device
/// through.
///
/// One per backend was what the Vulkan loader could not survive. Enumerating
/// the physical devices of a fresh instance unloads the drivers that found
/// none, and the loader edits its lists of instances and drivers as it does —
/// while a device-level call from another thread, such as naming an object
/// for the validation layer, walks those same lists to find its driver. Two
/// outputs coming up at once, or the GPU tests run in parallel, crashed inside
/// the loader. Destroying an instance edits the lists the same way. With one
/// instance, made once and never destroyed, the drivers are sorted out once,
/// before anything has a device to call through.
fn shared_instance() -> wgpu::Instance {
    static INSTANCE: std::sync::OnceLock<wgpu::Instance> = std::sync::OnceLock::new();
    INSTANCE
        .get_or_init(|| {
            let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
            descriptor.backends = wgpu::Backends::VULKAN | wgpu::Backends::GL;
            wgpu::Instance::new(descriptor)
        })
        .clone()
}

/// One opened GPU device, which every backend on that adapter draws with.
///
/// Opening a device is most of what bringing a surface up costs -- a few
/// hundred milliseconds on a laptop's integrated GPU, for every layer
/// surface, popup and output -- and each one held its own copy of every
/// driver-side structure. The surfaces of one process draw on one GPU, so
/// they share one device and one queue; each keeps its own swapchain,
/// targets, pipelines and atlases.
#[derive(Clone)]
pub(crate) struct SharedDevice {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    dmabuf: Option<crate::gpu::dmabuf::DmabufSupport>,
    lcd_supported: bool,
}

/// The devices opened so far, one per adapter.
///
/// Usually one. A second appears only when a surface cannot be presented
/// from an adapter already open -- an output wired to another GPU -- which
/// is asked of each adapter before its device is reused.
fn shared_devices() -> &'static std::sync::Mutex<Vec<SharedDevice>> {
    static DEVICES: std::sync::OnceLock<std::sync::Mutex<Vec<SharedDevice>>> =
        std::sync::OnceLock::new();
    DEVICES.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

/// `MORF_GPU_SHARED=0`: every backend opens a device of its own, as before
/// devices were shared. For measuring the difference.
fn sharing_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| std::env::var("MORF_GPU_SHARED").map_or(true, |value| value != "0"))
}

/// How many devices this process has opened, for tests and diagnostics.
pub fn opened_device_count() -> usize {
    OPENED.load(std::sync::atomic::Ordering::Relaxed)
}

static OPENED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The device to draw `surface` with (or offscreen, without one): an open
/// one whose adapter can present to it, or a new one.
///
/// Two outputs coming up at once may both find nothing to share and open a
/// device each; the second to finish takes the first's instead, and its own
/// is dropped, so the process still ends up with one.
async fn device_for(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'static>>,
) -> Result<SharedDevice, GpuError> {
    if !sharing_wanted() {
        return open_device(instance, surface).await;
    }
    let reusable = |devices: &[SharedDevice]| {
        devices
            .iter()
            .find(|shared| {
                surface.is_none_or(|surface| shared.adapter.is_surface_supported(surface))
            })
            .cloned()
    };
    let lock = || {
        shared_devices()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    };
    let found = reusable(&lock());
    if let Some(shared) = found {
        return Ok(shared);
    }
    let opened = open_device(instance, surface).await?;
    let mut devices = lock();
    if let Some(shared) = reusable(&devices) {
        return Ok(shared);
    }
    devices.push(opened.clone());
    Ok(opened)
}

async fn open_device(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'static>>,
) -> Result<SharedDevice, GpuError> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: surface,
            apply_limit_buckets: false,
        })
        .await
        .map_err(|error| GpuError(format!("no compatible GPU adapter: {error}")))?;
    let adapter_limits = adapter.limits();
    // Subpixel text blends each channel by its own coverage, which takes
    // a second fragment output to the blend unit. Asked for only where
    // the adapter has it; without it text stays greyscale.
    let lcd_supported = std::env::var_os("MORF_NO_DUAL_SOURCE").is_none()
        && adapter
            .features()
            .contains(wgpu::Features::DUAL_SOURCE_BLENDING);
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("morf device"),
        required_features: if lcd_supported {
            wgpu::Features::DUAL_SOURCE_BLENDING
        } else {
            wgpu::Features::empty()
        },
        required_limits: adapter_limits.clone(),
        ..Default::default()
    };
    // Through wgpu-hal when the device can export dmabufs, so the
    // extensions that need enabling at creation are enabled; through wgpu
    // as usual otherwise, which is the same device without them.
    let (device, queue, dmabuf) = match open_with_dmabuf(instance, &adapter, &descriptor) {
        Some((device, queue, support)) => (device, queue, Some(support)),
        None => {
            let (device, queue) = adapter
                .request_device(&descriptor)
                .await
                .map_err(|error| GpuError(format!("could not create GPU device: {error}")))?;
            (device, queue, None)
        }
    };
    // A validation error -- a surface a nested compositor briefly will not
    // configure, say -- is logged and the frame is skipped, rather than a
    // panic that takes the output's thread with it.
    device.on_uncaptured_error(std::sync::Arc::new(|error: wgpu::Error| {
        eprintln!("morf: gpu: {error}");
    }));
    OPENED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Ok(SharedDevice {
        adapter,
        device,
        queue,
        dmabuf,
        lcd_supported,
    })
}

impl WgpuBackend {
    /// Selects a Vulkan or GLES adapter and creates an offscreen render target.
    pub async fn new(width: u32, height: u32) -> Result<Self, GpuError> {
        Self::initialize(shared_instance(), None, width, height).await
    }

    /// Creates a renderer presenting to an owned native window target.
    pub async fn new_surface<T>(window: T, width: u32, height: u32) -> Result<Self, GpuError>
    where
        T: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static,
    {
        let instance = shared_instance();
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
            &create_shader_uniform_buffer(&device, morf_shader::HEADER_BYTES),
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
            lcd_supported,
            subpixel: None,
            lcd_pipeline: None,
            opaque_surface: false,
            glyph_layout,
            glyph_sampler,
            nearest_sampler,
            glyph_mask_atlas,
            glyph_color_atlas,
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
            elapsed: 0.0,
            images: ImageCache::default(),
            image_textures: HashMap::new(),
            path_textures: HashMap::new(),
            path_outlines: Default::default(),
            layer_pool: Default::default(),
            backdrops: Default::default(),
            text: TextSystem::new(),
            drawings: morf_svg::SvgOutlines::new(),
            texture,
            view,
            surface,
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
            super::target_format(self.blend),
        );
        // The pooled layer targets are surface-sized, so a resize retires them.
        self.layer_pool.clear();
        // So is the backdrops' scratch, and every region they were cut from.
        self.backdrops.entries.clear();
        self.backdrops.scratch = None;
        let viewport = [self.width as f32, self.height as f32, self.elapsed, 0.0];
        self.queue
            .write_buffer(&self.viewport_buffer, 0, bytemuck::cast_slice(&viewport));
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
        let format = super::target_format(blend);
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
        self.blur_pipeline = build_blur_pipeline(&self.device, &self.blur_layout, blend);
        self.field_pipeline = build_field_pipeline(
            &self.device,
            FieldPipeline {
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
        self.shaders.clear();
        self.effect_shaders.clear();
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
