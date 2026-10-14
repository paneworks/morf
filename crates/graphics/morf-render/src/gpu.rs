use crate::BlendSpace;

/// The format every target and pipeline of one blend space agrees on.
///
/// Linear blending renders into an sRGB target: shaders write linear light
/// and the hardware encodes on store and decodes on blend, so the blend
/// happens in linear light. Gamma blending renders into a plain target and
/// the shaders encode themselves, so what the blend unit sees -- and mixes --
/// are the sRGB-encoded values, as a browser or Qt mixes them.
pub(crate) fn target_format(blend: BlendSpace) -> wgpu::TextureFormat {
    match blend {
        BlendSpace::Linear => wgpu::TextureFormat::Rgba8UnormSrgb,
        BlendSpace::Srgb => wgpu::TextureFormat::Rgba8Unorm,
    }
}

/// The pipeline constants telling the field and glyph shaders which space
/// they write in.
pub(crate) fn blend_constants(blend: BlendSpace) -> &'static [(&'static str, f64)] {
    match blend {
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
pub mod dmabuf;
mod dmabuf_acquire;
#[cfg(test)]
mod dmabuf_ring_tests;
mod field_pass;
#[cfg(test)]
mod glyph_atlas_tests;
mod glyph_batch;
#[cfg(test)]
mod glyph_reuse_tests;
mod glyph_upload;
mod glyphs;
mod layer_pool;
#[cfg(test)]
mod layer_pool_tests;
mod layer_targets;
mod lcd_spans;
#[cfg(test)]
mod lcd_spans_tests;
#[cfg(test)]
mod lcd_tests;
mod pipelines;
mod present;
mod present_link;
mod present_stalls;
#[cfg(test)]
mod present_tests;
mod profile;
mod shader_registry;
mod shaders;
mod targets;
mod terminal_batch;
mod textures;
mod warm;

pub use backend_init::{lose_devices, opened_device_count};
pub use backend_types::*;
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
mod inline_source_tests;
#[cfg(test)]
mod mask_tests;
#[cfg(test)]
mod partial_tests;
#[cfg(test)]
mod path_tests;
mod readback;
#[cfg(test)]
mod shader_host_tests;
#[cfg(test)]
mod shader_instance_tests;
#[cfg(test)]
mod shader_language_tests;
#[cfg(test)]
mod shader_mode_tests;
#[cfg(test)]
mod shader_pipeline_tests;
#[cfg(test)]
mod shader_scene_tests;
#[cfg(test)]
mod shader_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod text_field_tests;
