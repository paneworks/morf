//! Editing text: a caret, a selection, and the history to take them back.
//!
//! Pure on purpose. Nothing here knows a font, a pixel or a key: it is a
//! string, two byte offsets into it and a stack of what it used to be, so
//! every rule about how text is edited can be tested as arithmetic. Where a
//! line starts on screen is the caller's business; this knows the lines the
//! text itself has.
//!
//! Every offset is a byte offset that falls on a grapheme boundary. One
//! arriving from outside — from a configuration, from a hit test — is moved
//! back to the boundary before it, so the caret can never stand inside a
//! letter that happens to be several code points.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// How much a field remembers to undo, unless it is told otherwise.
pub const DEFAULT_HISTORY: usize = 100;

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    text: String,
    cursor: usize,
    anchor: usize,
}

/// What the last edit was, so a run of the same one can be undone at once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Run {
    Typing,
    Erasing,
    Deleting,
}

/// A string being edited.
#[derive(Clone, Debug)]
pub struct EditBuffer {
    text: String,
    cursor: usize,
    anchor: usize,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    run: Option<Run>,
    history: usize,
    /// The most characters the text may hold; zero is no limit.
    pub max_length: usize,
    /// Whether the text may hold line breaks.
    pub multiline: bool,
}

impl Default for EditBuffer {
    fn default() -> Self {
        Self::new("")
    }
}

impl EditBuffer {
    /// A buffer holding `text`, with the caret at its end.
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            cursor: text.len(),
            anchor: text.len(),
            undo: Vec::new(),
            redo: Vec::new(),
            run: None,
            history: DEFAULT_HISTORY,
            max_length: 0,
            multiline: false,
        }
    }

    /// How many edits can be undone at most; the oldest are forgotten first.
    pub fn set_history_limit(&mut self, limit: usize) {
        self.history = limit;
        self.trim_history();
    }

    /// The text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The caret, as a byte offset.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Where the selection started; the caret when nothing is selected.
    pub fn anchor(&self) -> usize {
        self.anchor
    }

    /// The selected range, in order.
    pub fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    /// Whether anything is selected.
    pub fn has_selection(&self) -> bool {
        self.cursor != self.anchor
    }

    /// The selected text; empty when nothing is.
    pub fn selected_text(&self) -> &str {
        &self.text[self.selection()]
    }

    /// Whether there is anything to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is anything to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Replaces the text from outside the field, with the caret at its end.
    ///
    /// Not an edit: the history is cleared rather than extended, because the
    /// text it would take back to is one the configuration has replaced.
    pub fn set_text(&mut self, text: &str) {
        let room = if self.max_length == 0 {
            usize::MAX
        } else {
            self.max_length
        };
        self.text = self.accept(text, room);
        self.cursor = self.text.len();
        self.anchor = self.cursor;
        self.undo.clear();
        self.redo.clear();
        self.run = None;
    }

    /// Moves the caret, keeping the anchor when `extend` does.
    pub fn set_cursor(&mut self, position: usize, extend: bool) {
        self.cursor = self.boundary_at_or_before(position);
        if !extend {
            self.anchor = self.cursor;
        }
        self.run = None;
    }

    /// Selects from `start` to `end`; the caret goes to `end`.
    pub fn select(&mut self, start: usize, end: usize) {
        self.anchor = self.boundary_at_or_before(start);
        self.cursor = self.boundary_at_or_before(end);
        self.run = None;
    }

    /// Selects everything, the caret at the end.
    pub fn select_all(&mut self) {
        self.select(0, self.text.len());
    }

    /// Drops the selection where the caret is.
    pub fn deselect(&mut self) {
        self.anchor = self.cursor;
        self.run = None;
    }

    /// Types `input` over the selection, or at the caret.
    ///
    /// Line breaks become spaces in a single-line field, since a pasted
    /// paragraph is still text somebody wants; carriage returns become plain
    /// line breaks in a multi-line one. What does not fit under
    /// `max_length` is dropped from the end. Returns whether the text changed.
    pub fn insert(&mut self, input: &str) -> bool {
        let selection = self.selection();
        let room = if self.max_length == 0 {
            usize::MAX
        } else {
            let kept = self.text.chars().count() - self.text[selection.clone()].chars().count();
            self.max_length.saturating_sub(kept)
        };
        let input = self.accept(input, room);
        if input.is_empty() && selection.is_empty() {
            return false;
        }
        // A run of typing undoes as one step, broken at each space so an undo
        // takes back a word rather than a paragraph.
        let continues = self.run == Some(Run::Typing)
            && selection.is_empty()
            && !input.chars().any(char::is_whitespace);
        self.remember(!continues);
        self.text.replace_range(selection.clone(), &input);
        self.cursor = selection.start + input.len();
        self.anchor = self.cursor;
        self.run = Some(Run::Typing);
        true
    }

    /// Replaces a range as one undoable edit, the caret after what went in.
    ///
    /// For an input method, which says what to delete around the caret and
    /// what to put there, rather than typing.
    pub fn replace(&mut self, range: Range<usize>, input: &str) -> bool {
        let start = self.boundary_at_or_before(range.start);
        let end = self.boundary_at_or_before(range.end.max(range.start));
        self.anchor = start;
        self.cursor = end;
        self.run = None;
        self.insert(input) || start != end
    }

    /// Backspace: the selection, or the grapheme (or word) before the caret.
    pub fn backspace(&mut self, word: bool) -> bool {
        if self.has_selection() {
            return self.erase_selection();
        }
        let start = if word {
            self.previous_word(self.cursor)
        } else {
            self.previous_grapheme(self.cursor)
        };
        self.erase(start..self.cursor, Some(Run::Erasing))
    }

    /// Delete: the selection, or the grapheme (or word) after the caret.
    pub fn delete(&mut self, word: bool) -> bool {
        if self.has_selection() {
            return self.erase_selection();
        }
        let end = if word {
            self.next_word(self.cursor)
        } else {
            self.next_grapheme(self.cursor)
        };
        self.erase(self.cursor..end, Some(Run::Deleting))
    }

    /// Removes the selection, returning whether there was one.
    pub fn erase_selection(&mut self) -> bool {
        let selection = self.selection();
        // A selection removed is a step of its own, never part of a run.
        self.erase(selection, None)
    }

    fn erase(&mut self, range: Range<usize>, run: Option<Run>) -> bool {
        if range.is_empty() {
            return false;
        }
        let continues = run.is_some() && self.run == run && !self.has_selection();
        self.remember(!continues);
        self.text.replace_range(range.clone(), "");
        self.cursor = range.start;
        self.anchor = range.start;
        self.run = run;
        true
    }

    /// Left: one grapheme, or to the start of a word, or to the start of the
    /// selection when there is one and it is not being extended.
    pub fn move_left(&mut self, word: bool, extend: bool) {
        let target = if !extend && self.has_selection() && !word {
            self.selection().start
        } else if word {
            self.previous_word(self.cursor)
        } else {
            self.previous_grapheme(self.cursor)
        };
        self.set_cursor(target, extend);
    }

    /// Right: the mirror of [`EditBuffer::move_left`].
    pub fn move_right(&mut self, word: bool, extend: bool) {
        let target = if !extend && self.has_selection() && !word {
            self.selection().end
        } else if word {
            self.next_word(self.cursor)
        } else {
            self.next_grapheme(self.cursor)
        };
        self.set_cursor(target, extend);
    }

    /// Home: the start of the caret's line of text.
    pub fn move_line_start(&mut self, extend: bool) {
        self.set_cursor(self.line_start(self.cursor), extend);
    }

    /// End: the end of the caret's line of text.
    pub fn move_line_end(&mut self, extend: bool) {
        self.set_cursor(self.line_end(self.cursor), extend);
    }

    /// The very start of the text.
    pub fn move_start(&mut self, extend: bool) {
        self.set_cursor(0, extend);
    }

    /// The very end of the text.
    pub fn move_end(&mut self, extend: bool) {
        self.set_cursor(self.text.len(), extend);
    }

    /// Takes back the last edit. Returns whether there was one.
    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(snapshot);
        true
    }

    /// Puts back the last edit undone. Returns whether there was one.
    pub fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(snapshot);
        true
    }

    /// The word, or the run of space or punctuation, under an offset — what a
    /// double click selects.
    pub fn word_at(&self, position: usize) -> Range<usize> {
        let position = self.boundary_at_or_before(position);
        let mut last = None;
        for (start, segment) in self.text.split_word_bound_indices() {
            let range = start..start + segment.len();
            if range.contains(&position) {
                // A click at the very start of a word belongs to it, and one
                // just past a word's end to the word rather than the space.
                if position == range.start
                    && !is_word(segment)
                    && let Some((previous, true)) = last
                {
                    return previous;
                }
                return range;
            }
            last = Some((range, is_word(segment)));
        }
        last.map_or(position..position, |(range, _)| range)
    }

    /// The line of text an offset is on, without its line break.
    pub fn line_at(&self, position: usize) -> Range<usize> {
        self.line_start(position)..self.line_end(position)
    }

    /// The grapheme boundary before an offset.
    pub fn previous_grapheme(&self, position: usize) -> usize {
        let position = self.boundary_at_or_before(position);
        self.text[..position]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(start, _)| start)
    }

    /// The grapheme boundary after an offset.
    pub fn next_grapheme(&self, position: usize) -> usize {
        let position = self.boundary_at_or_before(position);
        self.text[position..]
            .graphemes(true)
            .next()
            .map_or(self.text.len(), |grapheme| position + grapheme.len())
    }

    /// The start of the word before an offset, past any space between.
    pub fn previous_word(&self, position: usize) -> usize {
        let position = self.boundary_at_or_before(position);
        self.text[..position]
            .split_word_bound_indices()
            .rev()
            .find(|(_, segment)| is_word(segment))
            .map_or(0, |(start, _)| start)
    }

    /// The end of the word after an offset, past any space between.
    pub fn next_word(&self, position: usize) -> usize {
        let position = self.boundary_at_or_before(position);
        self.text[position..]
            .split_word_bound_indices()
            .find(|(_, segment)| is_word(segment))
            .map_or(self.text.len(), |(start, segment)| {
                position + start + segment.len()
            })
    }

    /// Where the line of text holding an offset starts.
    pub fn line_start(&self, position: usize) -> usize {
        let position = position.min(self.text.len());
        self.text[..position]
            .rfind('\n')
            .map_or(0, |index| index + 1)
    }

    /// Where the line of text holding an offset ends, before its break.
    pub fn line_end(&self, position: usize) -> usize {
        let position = position.min(self.text.len());
        self.text[position..]
            .find('\n')
            .map_or(self.text.len(), |index| position + index)
    }

    /// The grapheme boundary at or before an offset, inside the text.
    pub fn boundary_at_or_before(&self, position: usize) -> usize {
        if position >= self.text.len() {
            return self.text.len();
        }
        let mut boundary = 0;
        for (start, _) in self.text.grapheme_indices(true) {
            if start > position {
                break;
            }
            boundary = start;
        }
        boundary
    }

    /// What of `input` this field takes: its line breaks as it holds them,
    /// and no more characters than `room`.
    fn accept(&self, input: &str, room: usize) -> String {
        let normalised = if self.multiline {
            input.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            input.replace("\r\n", " ").replace(['\r', '\n'], " ")
        };
        // Other control characters are keys, not text: a tab typed into a
        // single line, an escape a keyboard reported as text.
        let mut kept = String::with_capacity(normalised.len());
        let mut count = 0;
        for grapheme in normalised.graphemes(true) {
            if grapheme
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
            {
                continue;
            }
            let chars = grapheme.chars().count();
            if count + chars > room {
                break;
            }
            count += chars;
            kept.push_str(grapheme);
        }
        kept
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text;
        self.cursor = snapshot.cursor.min(self.text.len());
        self.anchor = snapshot.anchor.min(self.text.len());
        self.run = None;
    }

    /// Records the text before an edit, unless it continues the last one.
    fn remember(&mut self, new_step: bool) {
        self.redo.clear();
        if new_step || self.undo.is_empty() {
            self.undo.push(self.snapshot());
            self.trim_history();
        }
    }

    fn trim_history(&mut self) {
        if self.undo.len() > self.history {
            let excess = self.undo.len() - self.history;
            self.undo.drain(..excess);
        }
    }
}

/// Whether a word-boundary segment is a word rather than space or punctuation.
fn is_word(segment: &str) -> bool {
    segment.chars().any(char::is_alphanumeric)
}

#[cfg(test)]
#[path = "edit_tests.rs"]
mod tests;
