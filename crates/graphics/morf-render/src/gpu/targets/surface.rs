//! The window's swapchain: configuring it, and acquiring its next image.

use super::super::backend_types::GpuError;
use super::super::depth::{PresentFormat, Transfer};
use super::composite::{
    CompositePipeline, composite_view, create_composite_bind_group, create_composite_pipeline,
};

pub(crate) struct SurfaceState {
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) config: wgpu::SurfaceConfiguration,
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) texture_layout: wgpu::BindGroupLayout,
    pub(crate) sampler: wgpu::Sampler,
    pub(crate) bind_group: wgpu::BindGroup,
    /// Whether the surface asked to be reconfigured while a frame was still in
    /// hand, so it has to be done before the next one is acquired.
    pub(crate) stale: bool,
    /// Every format the surface offered, and the eight-bit one chosen first.
    pub(crate) formats: Vec<wgpu::TextureFormat>,
    pub(crate) eight: wgpu::TextureFormat,
    /// The colour spaces each format can be configured in (`swapchain_hdr`),
    /// and whether HDR is asked for, which lets a format offered only in
    /// HDR spaces count.
    pub(crate) spaces: Vec<(wgpu::TextureFormat, wgpu::SurfaceColorSpaces)>,
    pub(crate) hdr: bool,
}

impl SurfaceState {
    /// Whether the swapchain can present as `format`.
    pub(crate) fn takes(&self, format: PresentFormat) -> bool {
        !format.deep()
            || self.formats.contains(&format.texture_format())
            || (self.hdr && self.hdr_encoding(format).hdr())
    }

    /// The swapchain format that presents as `format`.
    pub(crate) fn format_for(&self, format: PresentFormat) -> wgpu::TextureFormat {
        if format.deep() {
            format.texture_format()
        } else {
            self.eight
        }
    }

    /// Configures the swapchain as `format`, and the composite into it.
    pub(crate) fn reformat(
        &mut self,
        device: &wgpu::Device,
        target: &wgpu::Texture,
        (format, color_space): (wgpu::TextureFormat, wgpu::SurfaceColorSpace),
        transfer: Transfer,
    ) {
        if self.config.format != format || self.config.color_space != color_space {
            self.config.format = format;
            self.config.color_space = color_space;
            self.stale = true;
        }
        let CompositePipeline {
            pipeline,
            layout,
            sampler,
        } = create_composite_pipeline(device, format, transfer);
        self.bind_group =
            create_composite_bind_group(device, &layout, &composite_view(target), &sampler);
        self.pipeline = pipeline;
        self.texture_layout = layout;
        self.sampler = sampler;
    }
}

pub(crate) fn create_surface_state(
    device: &wgpu::Device,
    adapter: &wgpu::Adapter,
    surface: wgpu::Surface<'static>,
    target_view: &wgpu::TextureView,
    width: u32,
    height: u32,
) -> Result<SurfaceState, GpuError> {
    let capabilities = surface.get_capabilities(adapter);
    let format = capabilities
        .formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .or_else(|| capabilities.formats.first().copied())
        .ok_or_else(|| GpuError::Unsupported("GPU surface exposes no texture format".to_owned()))?;
    // Mailbox, then immediate, then whatever there is: never FIFO by
    // choice. The shell already paces itself on the compositor's frame
    // callbacks, so FIFO buys nothing -- and on Wayland Mesa implements it
    // with the commit-timing protocol, stamping each frame with a target
    // time it works out from the pace of the last ones. A shell that paints
    // when something moves and rests when nothing does has no steady pace
    // to work out, the targets drift a tenth of a second ahead, the
    // compositor holds the frames until then, and motion arrives late and
    // in lumps.
    let preferred = [wgpu::PresentMode::Mailbox, wgpu::PresentMode::Immediate];
    // `MORF_PRESENT=mode=fifo|immediate|mailbox` overrides it, for comparing
    // the compositor's behaviour under each.
    let present = morf_value::env::flags("MORF_PRESENT");
    let asked = present.value("mode").and_then(|value| match value {
        "fifo" => Some(wgpu::PresentMode::Fifo),
        "immediate" => Some(wgpu::PresentMode::Immediate),
        "mailbox" => Some(wgpu::PresentMode::Mailbox),
        _ => None,
    });
    let present_mode = asked
        .into_iter()
        .chain(preferred.iter().copied())
        .find(|mode| capabilities.present_modes.contains(mode))
        .or_else(|| capabilities.present_modes.first().copied())
        .ok_or_else(|| {
            GpuError::Unsupported("GPU surface exposes no presentation mode".to_owned())
        })?;
    let alpha_mode = capabilities
        .alpha_modes
        .iter()
        .copied()
        .find(|mode| *mode == wgpu::CompositeAlphaMode::PreMultiplied)
        .or_else(|| capabilities.alpha_modes.first().copied())
        .ok_or_else(|| GpuError::Unsupported("GPU surface exposes no alpha mode".to_owned()))?;
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        color_space: wgpu::SurfaceColorSpace::Auto,
        width: width.max(1),
        height: height.max(1),
        present_mode,
        desired_maximum_frame_latency: 2,
        alpha_mode,
        view_formats: vec![],
    };
    surface.configure(device, &config);
    let CompositePipeline {
        pipeline,
        layout: texture_layout,
        sampler,
    } = create_composite_pipeline(device, format, Transfer::COPY);
    let bind_group = create_composite_bind_group(device, &texture_layout, target_view, &sampler);
    Ok(SurfaceState {
        stale: false,
        formats: capabilities.formats.clone(),
        eight: format,
        spaces: capabilities
            .format_capabilities
            .iter()
            .map(|offered| (offered.format, offered.color_spaces))
            .collect(),
        hdr: false,
        surface,
        config,
        pipeline,
        texture_layout,
        sampler,
        bind_group,
    })
}

/// The next image to draw into, or `None` if this frame should be skipped.
///
/// Not every unsuccessful acquisition is a failure, and wgpu says as much for
/// each one. A timeout means the compositor has not released a buffer yet, and
/// occlusion means the surface is not on screen to draw to; the documented
/// answer to both is to skip this frame and try the next. Treating them as
/// errors instead took the whole surface down — which is how a shell died with
/// `could not acquire GPU surface: Timeout` for what is, on a busy compositor
/// driving three large outputs, an ordinary event.
///
/// The genuinely broken states still error. The difference is that they are now
/// the ones wgpu describes that way.
pub(crate) fn acquire_frame(
    device: &wgpu::Device,
    surface: &mut SurfaceState,
) -> Result<Option<wgpu::SurfaceTexture>, GpuError> {
    // Anything the last frame asked for, done now — before a texture is in
    // hand rather than while one is, which is the only moment it is allowed.
    if surface.stale {
        surface.surface.configure(device, &surface.config);
        surface.stale = false;
    }
    match surface.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(frame) => Ok(Some(frame)),
        // Suboptimal hands back a frame that is perfectly drawable — it only
        // says the surface no longer matches the swapchain, which is what a
        // compositor reports when it rescales or rotates a window. So it is
        // drawn, and the reconfigure waits for the next acquire.
        //
        // Reconfiguring here instead is a validation error, and a fatal one:
        // wgpu requires the surface texture to be dropped first, and this still
        // held it. Nothing on the desk ever returned Suboptimal, so nothing
        // ever hit it; a phone whose compositor scales the window returns it on
        // the very first frame and the process aborts before drawing anything.
        wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
            surface.stale = true;
            Ok(Some(frame))
        }
        wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => Ok(None),
        // Outdated wants a reconfigure; lost wants the surface recreated, which
        // needs a window handle this layer does not hold — so it gets the
        // reconfigure too, because attempting it and failing is no worse than
        // failing immediately and is sometimes enough.
        status @ (wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost) => {
            surface.surface.configure(device, &surface.config);
            match surface.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame)
                | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Ok(Some(frame)),
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    Ok(None)
                }
                after => Err(GpuError::Surface(format!(
                    "could not acquire GPU surface after {status:?}: {after:?}"
                ))),
            }
        }
        status => Err(GpuError::Surface(format!(
            "could not acquire GPU surface: {status:?}"
        ))),
    }
}
