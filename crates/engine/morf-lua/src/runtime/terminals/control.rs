//! A terminal's methods: writing and pasting to its program, signalling it,
//! scrolling its history, and reading back its text, selection and pid.

use super::*;

/// Writes to a terminal's program.
pub(crate) fn write(
    state: &mut ReactiveState,
    node: NodeHandle,
    bytes: Vec<u8>,
) -> Result<(), String> {
    let entry = state
        .terminals
        .entries
        .get_mut(&node)
        .ok_or_else(|| "not a terminal".to_owned())?;
    entry.write(bytes)
}

/// Pastes text into a terminal, bracketed when its program asked for that.
pub(crate) fn paste(state: &mut ReactiveState, node: NodeHandle, text: &str) -> Result<(), String> {
    let entry = state
        .terminals
        .entries
        .get_mut(&node)
        .ok_or_else(|| "not a terminal".to_owned())?;
    entry.emulator.scroll_to_bottom();
    let bytes = entry.emulator.encode_paste(text);
    entry.write(bytes)
}

/// Signals a terminal's program. Returns whether it was running.
pub(crate) fn kill(state: &mut ReactiveState, node: NodeHandle, signal: i32) -> bool {
    match state.terminals.entries.get(&node) {
        Some(TerminalEntry {
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

/// Moves a terminal's view through its history. Returns whether it moved.
pub(crate) fn scroll(state: &mut ReactiveState, node: NodeHandle, lines: i32) -> bool {
    let Some(entry) = state.terminals.entries.get_mut(&node) else {
        return false;
    };
    let moved = if lines == 0 {
        entry.emulator.scroll_to_bottom()
    } else {
        entry.emulator.scroll(lines)
    };
    if moved {
        refresh_screen(state, node);
    }
    moved
}

/// What a terminal shows, as text.
pub(crate) fn text(state: &ReactiveState, node: NodeHandle) -> Option<String> {
    Some(state.terminals.entries.get(&node)?.emulator.text())
}

/// The text selected with the pointer, if any.
pub(crate) fn selection(state: &ReactiveState, node: NodeHandle) -> Option<String> {
    state
        .terminals
        .entries
        .get(&node)?
        .emulator
        .selection_text()
}

/// Drops the selection. Whether there was one.
pub(crate) fn clear_selection(state: &mut ReactiveState, node: NodeHandle) -> bool {
    let Some(entry) = state.terminals.entries.get_mut(&node) else {
        return false;
    };
    let had = entry.emulator.select_clear();
    if had {
        refresh_screen(state, node);
    }
    had
}

/// The process id of a terminal's program, while it runs.
pub(crate) fn pid(state: &ReactiveState, node: NodeHandle) -> Option<u32> {
    let entry = state.terminals.entries.get(&node)?;
    if entry.exited {
        return None;
    }
    entry.pty.as_ref()?.pid()
}
