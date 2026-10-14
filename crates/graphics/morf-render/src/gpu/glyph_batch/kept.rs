//! The text a frame laid out, kept for the next.
//!
//! Positioning a label's glyphs is shaping it, if its buffer moved, and
//! walking every glyph for its raster: most of a frame's glyph batch, and the
//! same answer for a command copied unchanged from the last list. A copied
//! command's glyphs and bands are taken from the last batch instead, as long
//! as that batch was made from the list it was copied from, at the same
//! scale, over the same fonts -- and so are their quads, while the atlases
//! have moved nothing and the target kept its size (quads.rs).

use morf_text::TextSystem;

use super::PreparedBand;
use super::quads::{GLYPHS, KeptQuads, OVER, UNDER};
use crate::gpu::glyphs::{GlyphBatch, GlyphInstance, PreparedGlyph};
use crate::{CopiedRun, Copies, DrawCommand, RunCursor, extend_runs};

/// What one frame's commands laid out, in command order.
#[derive(Default)]
pub(crate) struct KeptText {
    /// The frame whose list this was made from, while it can be read.
    frame: Option<u64>,
    scale_120: u32,
    fonts: usize,
    glyphs: Vec<PreparedGlyph>,
    under: Vec<PreparedBand>,
    over: Vec<PreparedBand>,
    /// The quads they were drawn as, once the frame is drawn.
    quads: Option<Quads>,
}

/// A frame's quads, and the atlases and target they were made for.
struct Quads {
    atlases: [u64; 2],
    target: (u32, u32),
    instances: Vec<GlyphInstance>,
    plain: Vec<bool>,
}

/// One frame's laid-out text on its way to the batch: its glyphs, the bands
/// beneath them and the bands over them.
pub(crate) type Laid<'a> = (
    &'a mut Vec<PreparedGlyph>,
    &'a mut Vec<PreparedBand>,
    &'a mut Vec<PreparedBand>,
);

/// Reads the kept text of the commands a list copied.
pub(crate) struct Copier<'a> {
    kept: &'a KeptText,
    cursor: RunCursor<'a>,
    /// Per kind of item, the ones copied: `to` this frame's, `from` the kept.
    items: [Vec<CopiedRun>; 3],
}

impl KeptText {
    /// What the commands of `copies` can be read from, if anything.
    pub(crate) fn copier<'a>(
        &'a mut self,
        copies: Copies<'a>,
        scale_120: u32,
        text: &mut TextSystem,
    ) -> Option<Copier<'a>> {
        let usable = self.frame.is_some()
            && self.frame == copies.from
            && self.scale_120 == scale_120
            && self.fonts == font_count(text);
        self.frame = None;
        usable.then(|| Copier {
            kept: self,
            cursor: RunCursor::new(copies.runs),
            items: Default::default(),
        })
    }

    /// Keeps what `frame` laid out, unless the fonts changed while it did.
    pub(crate) fn keep(
        &mut self,
        (glyphs, under, over): Laid<'_>,
        frame: Option<u64>,
        scale_120: u32,
        fonts: (usize, usize),
    ) {
        self.frame = frame.filter(|_| fonts.0 == fonts.1);
        self.scale_120 = scale_120;
        self.fonts = fonts.1;
        std::mem::swap(&mut self.glyphs, glyphs);
        std::mem::swap(&mut self.under, under);
        std::mem::swap(&mut self.over, over);
        self.quads = None;
    }

    /// Keeps the quads the text just kept was drawn as, into `target`.
    pub(crate) fn keep_quads(&mut self, batch: GlyphBatch, target: (u32, u32)) {
        let count = self.glyphs.len() + self.under.len() + self.over.len();
        self.quads =
            (batch.instances.len() == count && batch.plain.len() == count).then(|| Quads {
                atlases: batch.atlases,
                target,
                instances: batch.instances,
                plain: batch.plain,
            });
    }
}

/// How many faces the text system knows: loading one can change what any
/// label falls back to.
pub(crate) fn font_count(text: &mut TextSystem) -> usize {
    text.rasterizer().0.db().len()
}

impl Copier<'_> {
    /// Appends what command `index` laid out last frame, if it was copied
    /// from there; false, having appended nothing, when it has to be laid
    /// out again.
    pub(super) fn copy(&mut self, index: usize, command: &DrawCommand, laid: Laid<'_>) -> bool {
        let Some(source) = self.cursor.source(index) else {
            return false;
        };
        // A text input's buffer is the one its caret is read from: it is
        // shaped every frame it is drawn.
        if matches!(command, DrawCommand::Text { edit: Some(_), .. }) {
            return false;
        }
        let kept = self.kept;
        let glyphs = of_command(&kept.glyphs, source, |glyph| glyph.command_index);
        extend_runs(
            &mut self.items[GLYPHS],
            laid.0.len(),
            glyphs.start,
            glyphs.len(),
        );
        laid.0
            .extend(kept.glyphs[glyphs].iter().map(|glyph| PreparedGlyph {
                command_index: index,
                ..glyph.clone()
            }));
        for (kind, from, to) in [(UNDER, &kept.under, laid.1), (OVER, &kept.over, laid.2)] {
            let bands = of_command(from, source, |band| band.command_index);
            extend_runs(&mut self.items[kind], to.len(), bands.start, bands.len());
            to.extend(from[bands].iter().map(|band| PreparedBand {
                command_index: index,
                ..*band
            }));
        }
        true
    }

    /// The kept quads of what was copied, if they were made for these
    /// atlases, as they stand, and this target.
    pub(crate) fn quads(&self, atlases: [u64; 2], target: (u32, u32)) -> Option<KeptQuads<'_>> {
        let kept = self.kept;
        let quads = kept
            .quads
            .as_ref()
            .filter(|quads| quads.atlases == atlases && quads.target == target)?;
        let mut bases = [0; 3];
        bases[UNDER] = 0;
        bases[GLYPHS] = kept.under.len();
        bases[OVER] = kept.under.len() + kept.glyphs.len();
        Some(KeptQuads {
            instances: &quads.instances,
            plain: &quads.plain,
            bases,
            runs: &self.items,
        })
    }
}

/// Where the items of command `index` are, out of items in command order.
fn of_command<T>(
    items: &[T],
    index: usize,
    command: impl Fn(&T) -> usize,
) -> std::ops::Range<usize> {
    let start = items.partition_point(|item| command(item) < index);
    start..start + items[start..].partition_point(|item| command(item) == index)
}
