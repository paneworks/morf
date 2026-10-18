//! The composite pass: the persistent target copied onto what is presented,
//! and the views of the target it reads.

use super::super::shaders::fullscreen_source;

/// The pass that copies the persistent target onto what is presented, and
/// what it binds.
pub(crate) struct CompositePipeline {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) layout: wgpu::BindGroupLayout,
    pub(crate) sampler: wgpu::Sampler,
}

/// A composite pass writing `format`, converting as `transfer` says (see
/// `depth::transfer`).
pub(crate) fn create_composite_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    transfer: super::super::depth::Transfer,
) -> CompositePipeline {
    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("morf composite texture layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("morf composite sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("morf composite pipeline layout"),
        bind_group_layouts: &[Some(&texture_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("morf composite shader"),
        source: wgpu::ShaderSource::Wgsl(
            fullscreen_source(include_str!("../../composite.wgsl")).into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("morf composite pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &transfer.constants(),
                ..Default::default()
            },
        }),
        multiview_mask: None,
        cache: None,
    });
    CompositePipeline {
        pipeline,
        layout: texture_layout,
        sampler,
    }
}

/// The view of the persistent target the surface composite samples.
///
/// The swapchain is sRGB, so it encodes what the composite writes. A linear
/// target decodes on read and the two cancel. A gamma-blended target already
/// holds encoded values; read through an sRGB view they are decoded, and the
/// swapchain's encode puts back exactly the bytes that were blended.
pub(crate) fn composite_view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(texture.format().add_srgb_suffix()),
        ..Default::default()
    })
}

/// The persistent target read as the bytes it holds, with no decoding:
/// what a composite into a plain 8-bit buffer copies. Either blend space
/// stores sRGB-encoded bytes, which is what a compositor wants.
pub(crate) fn plain_view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(texture.format().remove_srgb_suffix()),
        ..Default::default()
    })
}

pub(crate) fn create_composite_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("morf composite bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}
