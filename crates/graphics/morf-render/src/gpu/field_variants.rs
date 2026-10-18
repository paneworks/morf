//! The field shader's specialised pipelines, each built the first time a
//! frame draws with it.
//!
//! Every variant is the 1100-line field shader compiled again with other
//! constants. Building them all up front, and again when a configuration
//! sets its blend space, put seconds on the first frame of a shell that
//! draws with two of them; a cell left empty costs nothing.

use std::sync::OnceLock;

use super::field_pass::{FieldPipeline, FieldVariant, build_field_pipeline};
use super::{Space, WgpuBackend};

/// One cell per specialised variant; `General` is the backend's own field
/// pipeline and is never kept here.
#[derive(Default)]
pub(crate) struct FieldVariants {
    cells: [OnceLock<wgpu::RenderPipeline>; 7],
}

impl FieldVariants {
    fn slot(variant: FieldVariant) -> usize {
        match variant {
            FieldVariant::General => unreachable!("the general field pipeline is not a variant"),
            FieldVariant::Analytic => 0,
            FieldVariant::AnalyticOpaque => 1,
            FieldVariant::Boxes => 2,
            FieldVariant::BoxesOpaque => 3,
            FieldVariant::Quad => 4,
            FieldVariant::BoxesUniform => 5,
            FieldVariant::OneBox => 6,
        }
    }

    /// Forgets every built variant: the next frame builds for the new space.
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// How many variants have been built.
    #[cfg(test)]
    pub(crate) fn built(&self) -> usize {
        self.cells
            .iter()
            .filter(|cell| cell.get().is_some())
            .count()
    }
}

impl WgpuBackend {
    /// The pipeline for `variant`, built now if no frame has used it yet.
    pub(crate) fn field_variant(&self, variant: FieldVariant) -> &wgpu::RenderPipeline {
        if variant == FieldVariant::General {
            return &self.field_pipeline;
        }
        self.field_variants.cells[FieldVariants::slot(variant)]
            .get_or_init(|| build_variant(self, variant, self.space))
    }
}

fn build_variant(
    backend: &WgpuBackend,
    variant: FieldVariant,
    space: Space,
) -> wgpu::RenderPipeline {
    build_field_pipeline(
        &backend.device,
        FieldPipeline {
            layout: &backend.field_layout,
            shader_layout: &backend.field_shader_layout,
            variant,
            user: None,
            owns_coverage: false,
            vertex: None,
            textures: None,
            data: None,
            space,
        },
    )
    .expect("the field shader carries its own hook")
}
