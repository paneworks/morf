//! The fields a frame gathered, kept for the next.
//!
//! A command copied unchanged from the last list is drawn as the same
//! instances, layers, materials and outlines it was drawn as last frame,
//! only at other places in the buffers: what refers to a place is moved
//! with it -- an instance's first layer and material, a polygon layer's
//! first outline point.

use morf_text::TextSystem;

use super::FieldBatch;
use crate::gpu::glyph_batch::font_count;
use crate::{Copies, RunCursor};

/// What one frame's fields were gathered as, and what from.
#[derive(Default)]
pub(crate) struct KeptFields {
    /// The frame whose list this was made from, while it can be read.
    frame: Option<u64>,
    scale_120: u32,
    fonts: usize,
    batch: FieldBatch,
}

/// Reads the kept fields of the commands a list copied.
pub(crate) struct FieldCopier<'a> {
    kept: &'a FieldBatch,
    cursor: RunCursor<'a>,
}

impl KeptFields {
    /// What the commands of `copies` can be read from, if anything.
    pub(crate) fn copier<'a>(
        &'a mut self,
        copies: Copies<'a>,
        scale_120: u32,
        text: &mut TextSystem,
    ) -> Option<FieldCopier<'a>> {
        let usable = self.frame.is_some()
            && self.frame == copies.from
            && self.scale_120 == scale_120
            && self.fonts == font_count(text)
            && self.batch.starts.len() == self.batch.indices.len() + 1;
        self.frame = None;
        usable.then_some(FieldCopier {
            kept: &self.batch,
            cursor: RunCursor::new(copies.runs),
        })
    }

    /// Keeps what `frame` gathered, unless the fonts changed while it did.
    pub(crate) fn keep(
        &mut self,
        batch: FieldBatch,
        frame: Option<u64>,
        scale_120: u32,
        fonts: (usize, usize),
    ) {
        self.frame = frame.filter(|_| fonts.0 == fonts.1);
        self.scale_120 = scale_120;
        self.fonts = fonts.1;
        self.batch = batch;
    }
}

impl FieldCopier<'_> {
    /// Appends what command `index` was gathered as last frame, if it was
    /// copied from there; false, having appended nothing, when it has to be
    /// gathered again.
    pub(super) fn copy(&mut self, index: usize, batch: &mut FieldBatch) -> bool {
        let Some(source) = self.cursor.source(index) else {
            return false;
        };
        let kept = self.kept;
        let (Some(from), Some(until)) = (kept.starts.get(source), kept.starts.get(source + 1))
        else {
            return false;
        };
        let [layers, materials, outlines] = *from;
        let to = [
            batch.layers.len(),
            batch.materials.len(),
            batch.outlines.len(),
        ];
        let moved = |at: f32, from: usize, to: usize| (at as usize - from + to) as f32;
        batch
            .layers
            .extend(kept.layers[layers..until[0]].iter().map(|layer| {
                let mut layer = *layer;
                // A polygon layer's outline count is set; the start beside it
                // is then a place in the outlines.
                if layer.extra[3] > 0.0 {
                    layer.params[0] = moved(layer.params[0], outlines, to[2]);
                }
                layer
            }));
        batch
            .materials
            .extend_from_slice(&kept.materials[materials..until[1]]);
        batch
            .outlines
            .extend_from_slice(&kept.outlines[outlines..until[2]]);
        if let Some(range) = kept.indices[source].clone() {
            let start = batch.instances.len() as u32;
            let (from, until) = (range.start as usize, range.end as usize);
            batch
                .instances
                .extend(kept.instances[from..until].iter().map(|instance| {
                    let mut instance = *instance;
                    instance.style[2] = moved(instance.style[2], layers, to[0]);
                    instance.transform_offset[2] =
                        moved(instance.transform_offset[2], materials, to[1]);
                    instance
                }));
            batch.shaders.extend_from_slice(&kept.shaders[from..until]);
            batch.indices[index] = Some(start..batch.instances.len() as u32);
        }
        true
    }
}
