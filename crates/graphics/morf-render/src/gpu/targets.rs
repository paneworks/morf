//! What frames are drawn into: the persistent target and its blur chain,
//! the swapchain it is composited onto, and that composite.

use wgpu::util::DeviceExt;

use super::textures::{BlurChain, BlurPass};

mod composite;
mod damage_rects;
mod surface;

pub(crate) use composite::{
    CompositePipeline, composite_view, create_composite_bind_group, create_composite_pipeline,
    plain_view,
};
pub(crate) use damage_rects::{
    clamp_scissor, grow_damage, intersect_damage, placed_scissor, quad_reach, union_damage,
};
pub(crate) use surface::{SurfaceState, acquire_frame, create_surface_state};

pub(crate) fn create_target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> (wgpu::Texture, wgpu::TextureView) {
    let twins = [format.add_srgb_suffix(), format.remove_srgb_suffix()];
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("morf persistent target"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        // A gamma-blended target is also read through an sRGB view, by the
        // pass that hands it to an sRGB swapchain; see `composite_view`. And
        // every target through a plain one, by the pass that copies its bytes
        // into a buffer of this engine's own; see `plain_view`. A half-float
        // target has no sRGB twin and is only ever read as itself.
        view_formats: if twins[0] == twins[1] { &[] } else { &twins },
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

pub(crate) fn create_blur_chain(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    (texture, source): (&wgpu::Texture, &wgpu::TextureView),
    offset: f32,
    mut target: impl FnMut((u32, u32)) -> (wgpu::Texture, wgpu::TextureView),
) -> BlurChain {
    // The chain matches its source: its size, and the format of the blend
    // space it was rendered in.
    let (width, height) = (texture.width(), texture.height());
    let half = ((width / 2).max(1), (height / 2).max(1));
    let quarter = ((width / 4).max(1), (height / 4).max(1));
    let sizes = [half, quarter, half, (width.max(1), height.max(1))];
    let mut textures = Vec::with_capacity(4);
    let mut views = Vec::with_capacity(4);
    for (target_width, target_height) in sizes {
        let (texture, view) = target((target_width, target_height));
        textures.push(texture);
        views.push(view);
    }
    let sources = [source, &views[0], &views[1], &views[2]];
    let source_sizes = [(width.max(1), height.max(1)), half, quarter, half];
    let mut passes = Vec::with_capacity(4);
    for index in 0..4 {
        passes.push(create_blur_pass(
            device,
            layout,
            sampler,
            sources[index],
            source_sizes[index],
            offset,
            if index < 2 { 0.0 } else { 1.0 },
        ));
    }
    BlurChain {
        _textures: textures,
        views,
        passes,
    }
}

pub(crate) fn create_blur_pass(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    source: &wgpu::TextureView,
    source_size: (u32, u32),
    offset: f32,
    mode: f32,
) -> BlurPass {
    let params = [
        1.0 / source_size.0.max(1) as f32,
        1.0 / source_size.1.max(1) as f32,
        offset,
        mode,
    ];
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("morf blur parameters"),
        contents: bytemuck::cast_slice(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("morf blur bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: buffer.as_entire_binding(),
            },
        ],
    });
    BlurPass {
        _params: buffer,
        bind_group,
    }
}
