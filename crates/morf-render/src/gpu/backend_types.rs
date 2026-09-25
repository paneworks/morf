use morf_image::ImageCache;
use morf_text::TextSystem;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt;

use super::{glyphs::*, targets::*, textures::*};

/// Adapter selected for the wgpu renderer.
/// A published GPU texture: the image it wraps, and its view and bind group,
/// made once because the texture never changes size or format.
pub(crate) struct ExternalTexture {
    pub(crate) image: crate::gpu::dmabuf::DmabufImage,
    pub(crate) bind_group: wgpu::BindGroup,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuInfo {
    /// Human-readable driver adapter name.
    pub name: String,
    /// Active wgpu backend.
    pub backend: wgpu::Backend,
    /// PCI vendor identifier when available.
    pub vendor: u32,
    /// PCI device identifier when available.
    pub device: u32,
    /// Whether this device can export images as dmabufs, for zero-copy
    /// capture. False on GLES, and on a Vulkan driver without the extensions.
    pub dmabuf: bool,
}

/// wgpu initialization or submission failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuError(pub(crate) String);

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl StdError for GpuError {}

/// wgpu SDF renderer targeting a persistent texture.
/// Everything the host knows about one shader when it registers it.
///
/// A struct rather than nine arguments: the surface grew a field per section of
/// the coverage plan, and a call site with nine positional booleans and slices
/// is a call site nobody can read.
pub struct ShaderRegistration<'a> {
    /// Hash of the generated WGSL, which is what a node carries.
    pub program: u64,
    /// The fragment or material shader, if there is one.
    pub wgsl: Option<&'a str>,
    /// The vertex displacement, if there is one.
    pub vertex: Option<&'a str>,
    /// Byte offset of each parameter in the uniform block.
    pub offsets: &'a [u32],
    pub uniform_size: u32,
    /// Whether the shader decides its own coverage.
    pub owns_coverage: bool,
    /// Whether it reads what is underneath, and so runs in the composite pass.
    pub effect: bool,
    /// Image paths for the textures it declared, in binding order.
    pub textures: &'a [String],
    /// Element counts for the data blocks it declared, in binding order.
    pub data: &'a [(String, u32)],
}

/// One registered shader: its pipeline, and the buffer its parameters go in.
pub(crate) struct ShaderProgram {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) uniforms: wgpu::Buffer,
    pub(crate) bind_group: wgpu::BindGroup,
    /// Byte offsets of each parameter, from the compiler, so the host and the
    /// shader cannot disagree about the layout.
    pub(crate) offsets: Vec<u32>,
    pub(crate) size: u32,
    /// The shader's own textures, if it declared any.
    pub(crate) textures: Option<wgpu::BindGroup>,
    /// Its data blocks: the buffers to write and the group to bind.
    pub(crate) data: Option<(Vec<wgpu::Buffer>, wgpu::BindGroup)>,
}

pub struct WgpuBackend {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    /// The space this surface blends in; every target and pipeline follows it.
    pub(crate) blend: crate::BlendSpace,
    pub(crate) clear_pipeline: wgpu::RenderPipeline,
    /// What the clear pipeline is built from, kept to rebuild it when the
    /// blend space changes.
    pub(crate) clear_layout: wgpu::PipelineLayout,
    pub(crate) clear_shader: wgpu::ShaderModule,
    pub(crate) viewport_buffer: wgpu::Buffer,
    pub(crate) viewport_bind_group: wgpu::BindGroup,
    pub(crate) glyph_pipeline: wgpu::RenderPipeline,
    /// Whether the device was opened with dual-source blending, which
    /// subpixel text needs.
    pub(crate) lcd_supported: bool,
    /// Subpixel text as asked for, and the pipeline that draws it: both
    /// `None` while it is off or the device cannot.
    pub(crate) subpixel: Option<crate::SubpixelText>,
    pub(crate) lcd_pipeline: Option<wgpu::RenderPipeline>,
    /// The surface is declared opaque as a whole, so every pixel of it
    /// counts as opaque ground for subpixel text.
    pub(crate) opaque_surface: bool,
    pub(crate) glyph_layout: wgpu::BindGroupLayout,
    pub(crate) glyph_sampler: wgpu::Sampler,
    /// Nearest-texel sampling for images drawn with `smooth = false`.
    pub(crate) nearest_sampler: wgpu::Sampler,
    pub(crate) glyph_mask_atlas: GlyphAtlas,
    pub(crate) glyph_color_atlas: GlyphAtlas,
    pub(crate) blur_pipeline: wgpu::RenderPipeline,
    pub(crate) blur_layout: wgpu::BindGroupLayout,
    pub(crate) blur_sampler: wgpu::Sampler,
    pub(crate) glyph_buffer: wgpu::Buffer,
    pub(crate) glyph_capacity: usize,
    pub(crate) texture_buffer: wgpu::Buffer,
    pub(crate) texture_capacity: usize,
    pub(crate) field_pipeline: wgpu::RenderPipeline,
    pub(crate) field_layout: wgpu::BindGroupLayout,
    pub(crate) field_buffer: wgpu::Buffer,
    pub(crate) field_capacity: usize,
    pub(crate) field_layer_buffer: wgpu::Buffer,
    pub(crate) field_material_buffer: wgpu::Buffer,
    pub(crate) field_outline_capacity: usize,
    pub(crate) field_outline_buffer: wgpu::Buffer,
    pub(crate) field_material_capacity: usize,
    pub(crate) field_layer_capacity: usize,
    pub(crate) field_bind_group: wgpu::BindGroup,
    /// Layout for a shader's own uniform block, group one of every field
    /// pipeline whether or not it has a shader.
    pub(crate) field_shader_layout: wgpu::BindGroupLayout,
    /// The empty block bound when a node has no shader, so the base pipeline
    /// still has something at group one.
    pub(crate) field_shader_default: wgpu::BindGroup,
    /// Configuration shaders, by the hash of their generated WGSL.
    ///
    /// Filled at configuration load and never during a frame: building a
    /// pipeline takes tens of milliseconds, which a compositor cannot spend at
    /// paint time.
    pub(crate) shaders: HashMap<u64, ShaderProgram>,
    /// Effect shaders, which splice into the composite pass rather than the
    /// field pass and so need a pipeline built from a different shader.
    pub(crate) effect_shaders: HashMap<u64, ShaderProgram>,
    /// Seconds since the shell started, as shaders read it.
    ///
    /// Held here rather than passed through `render`, because the render
    /// signature belongs to the backend trait and every other backend would
    /// have to carry a clock it does not use.
    pub(crate) elapsed: f32,
    pub(crate) images: ImageCache,
    pub(crate) image_textures: HashMap<TextureKey, TextureImage>,
    /// `Path` outlines already drawn, by everything that shaped their pixels,
    /// and the path data already parsed.
    pub(crate) path_textures: HashMap<u64, TextureImage>,
    pub(crate) path_outlines: crate::path::PathOutlines,
    /// Offscreen layer targets, kept between frames and handed to whichever
    /// layer fits: each layer is rendered only over what is read of it, into
    /// a texture about that size. See `layer_pool`.
    pub(crate) layer_pool: super::layer_pool::LayerPool,
    /// Frosted-glass backdrops: their blurred textures, kept until what is
    /// beneath them changes, and the scratch target they are drawn from.
    pub(crate) backdrops: super::backdrops::BackdropCache,
    pub(crate) text: TextSystem,
    /// Documents already read, so an icon is parsed and resampled once rather
    /// than once a frame.
    pub(crate) drawings: morf_svg::SvgOutlines,
    /// What the device said it could do about dmabufs, when it could.
    pub(crate) dmabuf: Option<crate::gpu::dmabuf::DmabufSupport>,
    /// Textures this engine did not decode: captures the compositor drew
    /// straight into GPU memory, published under a name `ui.Image` resolves
    /// as `gpu:<name>`. The GPU-side twin of `ImageCache`'s `memory:` images.
    pub(crate) external_textures: HashMap<String, ExternalTexture>,
    /// Exported images handed to a compositor and not yet drawn into, by
    /// capture request.
    ///
    /// Held here because the image is this device's: it cannot outlive the
    /// device, and nothing outside the backend should be asked to keep it.
    pub(crate) pending_exports: HashMap<u64, super::dmabuf::DmabufImage>,
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    pub(crate) surface: Option<SurfaceState>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) info: GpuInfo,
}
