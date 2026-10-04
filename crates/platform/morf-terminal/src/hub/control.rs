//! A terminal's methods: writing and pasting to its program, signalling it,
//! scrolling its history, and reading back its text, selection and pid.

use super::*;

impl<H> Hub<H> {
    /// Writes to a terminal's program.
    pub fn write(&mut self, node: NodeHandle, bytes: Vec<u8>) -> Result<(), String> {
        let entry = self
            .entries
            .get_mut(&node)
            .ok_or_else(|| "not a terminal".to_owned())?;
        entry.write(bytes)
    }

    /// Pastes text into a terminal, bracketed when its program asked for
    /// that.
    pub fn paste(&mut self, node: NodeHandle, text: &str) -> Result<(), String> {
        let entry = self
            .entries
            .get_mut(&node)
            .ok_or_else(|| "not a terminal".to_owned())?;
        entry.emulator.scroll_to_bottom();
        let bytes = entry.emulator.encode_paste(text);
        entry.write(bytes)
    }

    /// Signals a terminal's program. Returns whether it was running.
    pub fn kill(&self, node: NodeHandle, signal: i32) -> bool {
        match self.entries.get(&node) {
            Some(Entry {
                pty: Some(pty),
                exited: false,
                ..
            }) => {
                pty.signal(signal);
                true
            }
            _ => false,
        }
    }

    /// Moves a terminal's view through its history. Returns whether it
    /// moved.
    pub fn scroll(&mut self, scene: &mut Scene, node: NodeHandle, lines: i32) -> bool {
        let Some(entry) = self.entries.get_mut(&node) else {
            return false;
        };
        let moved = if lines == 0 {
            entry.emulator.scroll_to_bottom()
        } else {
            entry.emulator.scroll(lines)
        };
        if moved {
            self.refresh_screen(scene, node);
        }
        moved
    }

    /// What a terminal shows, as text.
    pub fn text(&self, node: NodeHandle) -> Option<String> {
        Some(self.entries.get(&node)?.emulator.text())
    }

    /// The text selected with the pointer, if any.
    pub fn selection(&self, node: NodeHandle) -> Option<String> {
        self.entries.get(&node)?.emulator.selection_text()
    }

    /// Drops the selection. Whether there was one.
    pub fn clear_selection(&mut self, scene: &mut Scene, node: NodeHandle) -> bool {
        let Some(entry) = self.entries.get_mut(&node) else {
            return false;
        };
        let had = entry.emulator.select_clear();
        if had {
            self.refresh_screen(scene, node);
        }
        had
    }

    /// The process id of a terminal's program, while it runs.
    pub fn pid(&self, node: NodeHandle) -> Option<u32> {
        let entry = self.entries.get(&node)?;
        if entry.exited {
            return None;
        }
        entry.pty.as_ref()?.pid()
    }
}
