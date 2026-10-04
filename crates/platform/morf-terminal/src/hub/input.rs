//! Keys, the pointer, the wheel and the keyboard focus, to a terminal.

use super::*;

impl<H> Hub<H> {
    /// Sends one key press to a terminal's program. Returns whether the
    /// terminal's picture changed (its cursor now solid, its view back at
    /// the bottom of the history).
    pub fn key(
        &mut self,
        scene: &mut Scene,
        node: NodeHandle,
        keysym: u32,
        text: Option<&str>,
        modifiers: Modifiers,
    ) -> bool {
        let focus_moved = self.focused != Some(node);
        let previous = self.focused.replace(node);
        let Some(entry) = self.entries.get_mut(&node) else {
            return false;
        };
        let mut changed = false;
        if let Some(bytes) = entry.emulator.encode_key(keysym, text, modifiers) {
            // Typing is about now: back to where the program is writing.
            changed |= entry.emulator.scroll_to_bottom();
            let _ = entry.write(bytes);
        }
        if focus_moved {
            if let Some(previous) = previous {
                self.refresh_screen(scene, previous);
            }
            changed = true;
        }
        changed | self.refresh_screen(scene, node)
    }

    /// Where a point inside the node is on its grid.
    fn cell_at(
        &self,
        scene: &Scene,
        node: NodeHandle,
        local: (f64, f64),
    ) -> Option<(usize, usize)> {
        self.cell_and_half_at(scene, node, local)
            .map(|(column, row, _)| (column, row))
    }

    /// The cell under a point, and whether the point is on its right half.
    fn cell_and_half_at(
        &self,
        scene: &Scene,
        node: NodeHandle,
        local: (f64, f64),
    ) -> Option<(usize, usize, bool)> {
        let entry = self.entries.get(&node)?;
        let metrics = entry.metrics?;
        let padding = scene.number(node, "padding").ok()?.max(0.0);
        let across = ((local.0 - padding) / metrics.cell_width).max(0.0);
        let row = ((local.1 - padding) / metrics.cell_height).floor().max(0.0) as usize;
        Some((
            (across.floor() as usize).min(entry.emulator.columns().saturating_sub(1)),
            row.min(entry.emulator.rows().saturating_sub(1)),
            across.fract() >= 0.5,
        ))
    }

    /// A pointer press, release or motion over a terminal: to the program
    /// when it asked for the pointer. A press also gives the terminal the
    /// keyboard.
    pub fn pointer(
        &mut self,
        scene: &mut Scene,
        node: NodeHandle,
        action: MouseAction,
        button: Option<u32>,
        local: (f64, f64),
    ) -> bool {
        let mut changed = false;
        if action == MouseAction::Press && self.focused != Some(node) {
            let previous = self.focused.replace(node);
            if let Some(previous) = previous {
                self.refresh_screen(scene, previous);
            }
            changed = true;
        }
        let Some((column, row, right_half)) = self.cell_and_half_at(scene, node, local) else {
            return changed;
        };
        let Some(entry) = self.entries.get_mut(&node) else {
            return changed;
        };
        // A program that did not ask for the pointer leaves the left button
        // to selecting: a drag selects cells, a double click a word, a
        // triple click a line.
        let left =
            button.is_none_or(|code| MouseButton::from_code(code) == Some(MouseButton::Left));
        if !entry.emulator.mouse_modes().any() && (entry.selecting || left) {
            match action {
                MouseAction::Press if left => {
                    let now = Instant::now();
                    let count = match entry.last_click {
                        Some((at, cell, count))
                            if cell == (column, row) && now.duration_since(at) < DOUBLE_CLICK =>
                        {
                            count % 3 + 1
                        }
                        _ => 1,
                    };
                    entry.last_click = Some((now, (column, row), count));
                    let kind = match count {
                        1 => SelectionKind::Cells,
                        2 => SelectionKind::Word,
                        _ => SelectionKind::Line,
                    };
                    entry.emulator.select_start(column, row, right_half, kind);
                    entry.selecting = true;
                }
                MouseAction::Motion if entry.selecting => {
                    entry.emulator.select_update(column, row, right_half);
                }
                MouseAction::Release if entry.selecting => {
                    entry.selecting = false;
                    entry.emulator.select_finish();
                }
                _ => return changed,
            }
            return changed | self.refresh_screen(scene, node);
        }
        let button = match action {
            MouseAction::Press => {
                let pressed = button
                    .and_then(MouseButton::from_code)
                    .unwrap_or(MouseButton::Left);
                entry.held = Some(pressed);
                pressed
            }
            MouseAction::Release => {
                let released = button
                    .and_then(MouseButton::from_code)
                    .or(entry.held)
                    .unwrap_or(MouseButton::Left);
                entry.held = None;
                released
            }
            MouseAction::Motion => entry.held.unwrap_or(MouseButton::None),
        };
        let held = entry.held.is_some();
        if let Some(bytes) =
            entry
                .emulator
                .encode_mouse(button, action, column, row, Modifiers::default(), held)
        {
            let _ = entry.write(bytes);
        }
        changed | self.refresh_screen(scene, node)
    }

    /// A wheel turn over a terminal: to the program when it asked for the
    /// pointer, as arrow keys to a full-screen program that did not, and
    /// otherwise through the history.
    pub fn wheel(
        &mut self,
        scene: &mut Scene,
        node: NodeHandle,
        local: (f64, f64),
        pixels: f64,
        steps: i32,
    ) -> bool {
        let cell = self.cell_at(scene, node, local);
        let Some(entry) = self.entries.get_mut(&node) else {
            return false;
        };
        // Whole lines: a wheel's detents three at a time, a touchpad's
        // pixels a cell's height at a time.
        let lines = if steps != 0 {
            steps * WHEEL_LINES
        } else {
            let height = entry.metrics.map_or(16.0, |metrics| metrics.cell_height);
            entry.wheel_rest += pixels;
            let whole = (entry.wheel_rest / height).trunc();
            entry.wheel_rest -= whole * height;
            whole as i32
        };
        if lines == 0 {
            return false;
        }
        let (column, row) = cell.unwrap_or((0, 0));
        if entry.emulator.mouse_modes().any() {
            let button = if lines < 0 {
                MouseButton::WheelUp
            } else {
                MouseButton::WheelDown
            };
            let mut bytes = Vec::new();
            for _ in 0..lines.unsigned_abs().min(12) {
                if let Some(report) = entry.emulator.encode_mouse(
                    button,
                    MouseAction::Press,
                    column,
                    row,
                    Modifiers::default(),
                    false,
                ) {
                    bytes.extend(report);
                }
            }
            let _ = entry.write(bytes);
            return false;
        }
        if entry.emulator.alternate_scroll() {
            let keysym = if lines < 0 { 0xff52 } else { 0xff54 };
            let mut bytes = Vec::new();
            for _ in 0..lines.unsigned_abs().min(24) {
                if let Some(key) = entry
                    .emulator
                    .encode_key(keysym, None, Modifiers::default())
                {
                    bytes.extend(key);
                }
            }
            let _ = entry.write(bytes);
            return false;
        }
        // Down the page is towards the bottom of the history.
        entry.emulator.scroll(-lines) && self.refresh_screen(scene, node)
    }

    /// Gives the keyboard to a terminal, or takes it from whichever had it.
    /// Returns whether that changed what a terminal's cursor looks like.
    pub fn set_focus(&mut self, scene: &mut Scene, node: Option<NodeHandle>) -> bool {
        if self.focused == node {
            return false;
        }
        let previous = std::mem::replace(&mut self.focused, node);
        let mut changed = false;
        for node in [previous, node].into_iter().flatten() {
            changed |= self.refresh_screen(scene, node);
        }
        changed
    }
}
