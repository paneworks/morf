//! The text a frame laid out, kept for the next.
//!
//! Positioning a label's glyphs is shaping it, if its buffer moved, and
//! walking every glyph for its raster: most of a frame's glyph batch, and the
//! same answer for a command copied unchanged from the last list. A copied
//! command's glyphs and bands are taken from the last batch instead, as long
//! as that batch was made from the list it was copied from, at the same
//! scale, over the same fonts.

use morf_text::TextSystem;

use super::PreparedBand;
use crate::gpu::glyphs::PreparedGlyph;
use crate::{CopiedRun, Copies, DrawCommand};

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
    runs: &'a [CopiedRun],
    run: usize,
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
        usable.then_some(Copier {
            kept: self,
            runs: copies.runs,
            run: 0,
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
        while self
            .runs
            .get(self.run)
            .is_some_and(|run| run.to + run.len <= index)
        {
            self.run += 1;
        }
        let Some(run) = self.runs.get(self.run).filter(|run| run.to <= index) else {
            return false;
        };
        // A text input's buffer is the one its caret is read from: it is
        // shaped every frame it is drawn.
        if matches!(command, DrawCommand::Text { edit: Some(_), .. }) {
            return false;
        }
        let source = run.from + index - run.to;
        let kept = self.kept;
        let glyphs = of_command(&kept.glyphs, source, |glyph| glyph.command_index);
        laid.0.extend(glyphs.iter().map(|glyph| PreparedGlyph {
            command_index: index,
            ..glyph.clone()
        }));
        for (from, to) in [(&kept.under, laid.1), (&kept.over, laid.2)] {
            let bands = of_command(from, source, |band| band.command_index);
            to.extend(bands.iter().map(|band| PreparedBand {
                command_index: index,
                ..*band
            }));
        }
        true
    }
}

/// The items of command `index`, out of items in command order.
fn of_command<T>(items: &[T], index: usize, command: impl Fn(&T) -> usize) -> &[T] {
    let start = items.partition_point(|item| command(item) < index);
    let end = start + items[start..].partition_point(|item| command(item) == index);
    &items[start..end]
}
