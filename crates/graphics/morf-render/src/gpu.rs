use crate::BlendSpace;

/// What a surface's targets hold: the space it blends in, and whether they
/// are deeper than eight bits a channel (see `depth`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub(crate) struct Space {
    pub(crate) blend: BlendSpace,
    pub(crate) deep: bool,
}

/// The format every target and pipeline of one space agrees on.
///
/// Linear blending renders into an sRGB target: shaders write linear light
/// and the hardware encodes on store and decodes on blend, so the blend
/// happens in linear light. Gamma blending renders into a plain target and
/// the shaders encode themselves, so what the blend unit sees -- and mixes --
/// are the sRGB-encoded values, as a browser or Qt mixes them.
///
/// A deep space holds either in half floats: there is no ten-bit sRGB
/// format, and linear light in ten bits bands in the shadows. Its full alpha
/// is what translucent surfaces need.
pub(crate) fn target_format(space: Space) -> wgpu::TextureFormat {
    match space {
        Space { deep: true, .. } => wgpu::TextureFormat::Rgba16Float,
        Space {
            blend: BlendSpace::Linear,
            ..
        } => wgpu::TextureFormat::Rgba8UnormSrgb,
        Space {
            blend: BlendSpace::Srgb,
            ..
        } => wgpu::TextureFormat::Rgba8Unorm,
    }
}

/// The pipeline constants telling the field and glyph shaders which space
/// they write in.
pub(crate) fn blend_constants(space: Space) -> &'static [(&'static str, f64)] {
    match space.blend {
        BlendSpace::Linear => &[("MORF_GAMMA_BLEND", 0.0)],
        BlendSpace::Srgb => &[("MORF_GAMMA_BLEND", 1.0)],
    }
}

#[cfg(test)]
#[macro_use]
mod test_gpu;

mod backdrops;
mod backend_init;
mod backend_render;
mod backend_types;
mod batches;
mod capture;
mod clear_pipeline;
mod depth;
#[cfg(test)]
mod depth_tests;
pub mod dmabuf;
mod dmabuf_acquire;
#[cfg(test)]
mod dmabuf_ring_tests;
mod field_pass;
mod field_variants;
#[cfg(test)]
mod glyph_atlas_tests;
mod glyph_batch;
#[cfg(test)]
mod glyph_reuse_tests;
mod glyph_upload;
mod glyphs;
pub mod hdr;
#[cfg(test)]
mod hdr_tests;
#[cfg(test)]
mod kept_tests;
mod layer_pool;
#[cfg(test)]
mod layer_pool_tests;
mod layer_share;
#[cfg(test)]
mod layer_share_tests;
mod layer_targets;
mod lcd_spans;
#[cfg(test)]
mod lcd_spans_tests;
#[cfg(test)]
mod lcd_tests;
#[cfg(test)]
mod one_box_tests;
mod pipelines;
mod present;
mod present_link;
mod present_stalls;
#[cfg(test)]
mod present_tests;
mod profile;
mod shader_registry;
mod shaders;
mod snapshot;
mod swapchain_hdr;
mod targets;
mod terminal_batch;
mod textures;
mod warm;

pub use backend_init::{lose_devices, opened_device_count};
pub use backend_types::*;
pub use depth::OutputDepth;
pub use hdr::HdrRequest;
pub use present_stalls::{PresentStalls, present_stalls};
#[cfg(test)]
mod backdrop_tests;
#[cfg(test)]
mod clip_tests;
#[cfg(test)]
mod drawer_tests;
#[cfg(test)]
mod exit_tests;
#[cfg(test)]
mod field_agreement_tests;
#[cfg(test)]
mod field_color_tests;
#[cfg(test)]
mod field_damage_tests;
#[cfg(test)]
mod field_opacity_tests;
#[cfg(test)]
mod field_shape_tests;
#[cfg(test)]
mod field_tests;
#[cfg(test)]
mod glass_tests;
#[cfg(test)]
mod inline_source_tests;
#[cfg(test)]
mod mask_tests;
#[cfg(test)]
mod partial_tests;
#[cfg(test)]
mod path_tests;
mod readback;
#[cfg(test)]
mod shader_effect_tests;
#[cfg(test)]
mod shader_host_tests;
#[cfg(test)]
mod shader_instance_tests;
#[cfg(test)]
mod shader_language_tests;
#[cfg(test)]
mod shader_pipeline_tests;
#[cfg(test)]
mod shader_scene_tests;
#[cfg(test)]
mod shader_surface_tests;
#[cfg(test)]
mod shader_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod text_field_tests;
#[cfg(test)]
mod texture_culling_tests;
