//! What a `Terminal` node shows, as the renderer needs it.
//!
//! A terminal's grid is not a property: it is thousands of cells that change
//! a line at a time, and a property is one value that is compared whole and
//! animated. So the runtime keeps the emulator, builds this picture of its
//! screen when something on it changed, and hangs it on the node in a side
//! table, the way a shader is hung on one. The renderer reads it back when it
//! paints the node.
//!
//! Every line is shared (`Arc`), so a new picture after a change on one line
//! reuses the other lines as they were, and comparing two pictures — which
//! the damage tracker does every frame something on the surface moved — is a
//! pointer comparison per line for the lines nobody touched.

use std::sync::Arc;

/// Style bits of one cell.
pub mod cell_style {
    pub const BOLD: u8 = 1;
    pub const ITALIC: u8 = 1 << 1;
    pub const UNDERLINE: u8 = 1 << 2;
    pub const STRIKEOUT: u8 = 1 << 3;
    /// The first half of a character two cells wide.
    pub const WIDE: u8 = 1 << 4;
    /// The second half of one: drawn by the cell before it.
    pub const SPACER: u8 = 1 << 5;
    pub const DOUBLE_UNDERLINE: u8 = 1 << 6;
    pub const UNDERCURL: u8 = 1 << 7;
}

/// One cell: what is written in it and how, colours already resolved (the
/// palette looked up, inverse and dim applied).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TerminalCell {
    /// The character; a space for an empty cell.
    pub character: char,
    /// Combining characters that follow it, if any.
    pub combining: Option<Box<str>>,
    /// Text colour, sRGB with alpha.
    pub foreground: [u8; 4],
    /// Cell background, sRGB with alpha.
    pub background: [u8; 4],
    /// [`cell_style`] bits.
    pub style: u8,
}

impl TerminalCell {
    /// Whether nothing would be drawn for it but its background.
    pub fn is_blank(&self) -> bool {
        (self.character == ' ' || self.character == '\0') && self.combining.is_none()
    }
}

/// One row of cells.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TerminalLine {
    pub cells: Vec<TerminalCell>,
}

impl TerminalLine {
    /// The row as text, without what trails at the right.
    pub fn text(&self) -> String {
        let mut text = String::with_capacity(self.cells.len());
        for cell in &self.cells {
            if cell.style & cell_style::SPACER != 0 {
                continue;
            }
            text.push(if cell.character == '\0' {
                ' '
            } else {
                cell.character
            });
            if let Some(combining) = &cell.combining {
                text.push_str(combining);
            }
        }
        text.truncate(text.trim_end().len());
        text
    }
}

/// How the cursor is drawn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalCursorShape {
    Block,
    /// An outline: a block for a terminal without the keyboard.
    HollowBlock,
    Underline,
    Beam,
}

/// Where the cursor is and what it looks like.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalCursor {
    pub column: usize,
    pub row: usize,
    pub shape: TerminalCursorShape,
    pub color: [u8; 4],
    /// The colour of the character under a block cursor.
    pub text_color: [u8; 4],
    /// Two cells wide, over a wide character.
    pub wide: bool,
}

/// The size of one cell, in logical pixels, whole ones so the grid lands on
/// the same pixels in every row and column.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerminalMetrics {
    pub cell_width: f64,
    pub cell_height: f64,
    /// From the top of a cell down to the baseline.
    pub baseline: f64,
    /// From the baseline down to the top of an underline.
    pub underline_offset: f64,
    /// From the baseline up to the middle of a strikeout.
    pub strikeout_offset: f64,
    /// How thick either is.
    pub stroke: f64,
}

impl Eq for TerminalMetrics {}

/// One picture of a terminal's screen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerminalScreen {
    pub columns: usize,
    pub rows: usize,
    pub lines: Vec<Arc<TerminalLine>>,
    pub cursor: Option<TerminalCursor>,
    /// Behind every cell whose background is the default, and around the
    /// grid where the node is larger than a whole number of cells.
    pub background: [u8; 4],
    pub font_family: String,
    pub font_size: f64,
    /// Space between the node's edge and the grid, on every side.
    pub padding: f64,
    pub metrics: TerminalMetrics,
}

// Its numbers are sizes, never NaN; being `Eq` is what lets a comparison of two
// shared pictures stop at the pointer.
impl Eq for TerminalScreen {}

impl TerminalScreen {
    /// Everything on the screen as text, one line per row, trailing blanks
    /// trimmed from each row and blank rows from the end.
    pub fn text(&self) -> String {
        let mut lines: Vec<String> = self.lines.iter().map(|line| line.text()).collect();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines.join("\n")
    }
}
