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

mod backdrops;
mod backend_init;
mod backend_render;
mod backend_types;
mod batches;
mod capture;
mod clear_pipeline;
pub mod dmabuf;
mod dmabuf_acquire;
mod field_pass;
mod glyph_batch;
mod glyphs;
mod layer_targets;
mod pipelines;
mod shader_registry;
mod shaders;
mod targets;
mod terminal_batch;
mod textures;

pub use backend_types::*;
#[cfg(test)]
mod backdrop_tests;
#[cfg(test)]
mod clip_tests;
#[cfg(test)]
mod field_agreement_tests;
#[cfg(test)]
mod field_color_tests;
#[cfg(test)]
mod field_shape_tests;
#[cfg(test)]
mod field_tests;
#[cfg(test)]
mod inline_source_tests;
#[cfg(test)]
mod path_tests;
mod readback;
#[cfg(test)]
mod shader_host_tests;
#[cfg(test)]
mod shader_language_tests;
#[cfg(test)]
mod shader_mode_tests;
#[cfg(test)]
mod shader_scene_tests;
#[cfg(test)]
mod shader_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod text_field_tests;
