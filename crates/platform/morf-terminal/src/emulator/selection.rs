//! Selecting with the pointer: starting, extending and clearing a selection,
//! and the text it covers.

use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};

use super::{Emulator, SelectionKind, TerminalEvent};

impl Emulator {
    /// Starts a selection at a cell of the view; `right_half` is which half
    /// of the cell the pointer is on.
    pub fn select_start(
        &mut self,
        column: usize,
        row: usize,
        right_half: bool,
        kind: SelectionKind,
    ) {
        let kind = match kind {
            SelectionKind::Cells => SelectionType::Simple,
            SelectionKind::Word => SelectionType::Semantic,
            SelectionKind::Line => SelectionType::Lines,
        };
        let point = self.view_point(column, row);
        self.term.selection = Some(Selection::new(kind, point, side(right_half)));
        self.selection_moved();
    }

    /// Moves the selection's far end to a cell of the view.
    pub fn select_update(&mut self, column: usize, row: usize, right_half: bool) {
        let point = self.view_point(column, row);
        if let Some(selection) = self.term.selection.as_mut() {
            selection.update(point, side(right_half));
            self.selection_moved();
        }
    }

    /// Ends a drag: an empty selection (a plain click) is dropped, and one
    /// with text is announced as [`TerminalEvent::Selection`].
    pub fn select_finish(&mut self) {
        let empty = self
            .term
            .selection
            .as_ref()
            .is_none_or(|selection| selection.is_empty());
        if empty {
            self.select_clear();
        } else if let Some(text) = self.selection_text() {
            self.notices.push(TerminalEvent::Selection(text));
        }
    }

    /// Drops the selection. Whether there was one.
    pub fn select_clear(&mut self) -> bool {
        let had = self.term.selection.take().is_some();
        if had {
            self.selection_moved();
        }
        had
    }

    /// The selected text, if anything is selected.
    pub fn selection_text(&self) -> Option<String> {
        self.term
            .selection_to_string()
            .filter(|text| !text.is_empty())
    }

    fn view_point(&self, column: usize, row: usize) -> Point {
        let offset = self.display_offset() as i32;
        Point::new(
            Line(row.min(self.rows().saturating_sub(1)) as i32 - offset),
            Column(column.min(self.columns().saturating_sub(1))),
        )
    }

    fn selection_moved(&mut self) {
        self.changed = true;
        self.selection_moved = true;
    }
}

fn side(right_half: bool) -> Side {
    if right_half { Side::Right } else { Side::Left }
}
