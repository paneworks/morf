//! What each key does to a field.

use super::*;

/// The keys a text input knows, as X keysyms.
mod keysym {
    pub(super) const BACKSPACE: u32 = 0xff08;
    pub(super) const RETURN: u32 = 0xff0d;
    pub(super) const KP_ENTER: u32 = 0xff8d;
    pub(super) const ESCAPE: u32 = 0xff1b;
    pub(super) const DELETE: u32 = 0xffff;
    pub(super) const KP_DELETE: u32 = 0xff9f;
    pub(super) const INSERT: u32 = 0xff63;
    pub(super) const KP_INSERT: u32 = 0xff9e;
    pub(super) const HOME: u32 = 0xff50;
    pub(super) const LEFT: u32 = 0xff51;
    pub(super) const UP: u32 = 0xff52;
    pub(super) const RIGHT: u32 = 0xff53;
    pub(super) const DOWN: u32 = 0xff54;
    pub(super) const PAGE_UP: u32 = 0xff55;
    pub(super) const PAGE_DOWN: u32 = 0xff56;
    pub(super) const END: u32 = 0xff57;
    pub(super) const KP_HOME: u32 = 0xff95;
    pub(super) const KP_LEFT: u32 = 0xff96;
    pub(super) const KP_UP: u32 = 0xff97;
    pub(super) const KP_RIGHT: u32 = 0xff98;
    pub(super) const KP_DOWN: u32 = 0xff99;
    pub(super) const KP_PAGE_UP: u32 = 0xff9a;
    pub(super) const KP_PAGE_DOWN: u32 = 0xff9b;
    pub(super) const KP_END: u32 = 0xff9c;
}

/// What a key did to a field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyOutcome {
    /// The field used it.
    Handled,
    /// Not a key the field has any use for; whoever else wants it may have it.
    Ignored,
}

/// One key pressed while a field has the keyboard.
pub fn key(
    state: &mut impl EditHost,
    node: NodeHandle,
    keysym: u32,
    text: Option<&str>,
    modifiers: KeyModifiers,
) -> KeyOutcome {
    if !pull(state, node) {
        return KeyOutcome::Ignored;
    }
    let read_only = state.scene().bool_value(node, "read_only").unwrap_or(false);
    let password = state.scene().bool_value(node, "password").unwrap_or(false);
    let multiline = state.scene().bool_value(node, "multiline").unwrap_or(false);
    let KeyModifiers {
        ctrl, shift, alt, ..
    } = modifiers;
    let letter = char::from_u32(keysym)
        .filter(char::is_ascii_alphabetic)
        .map(|letter| letter.to_ascii_lowercase());
    let mut edited = false;
    let mut vertical = false;
    let outcome = {
        let shown = display(state.scene(), node);
        let map = caret_map(state, node, &shown);
        let page = state
            .editing()
            .inputs
            .get(&node)
            .and_then(|input| input.geometry)
            .map_or(1, |geometry| {
                let line = map.lines().first().map_or(1.0, |line| line.height.max(1.0));
                ((geometry.height as f32 / line).floor() as isize).max(1)
            });
        let clipboard = state.clipboard_text();
        let editing = state.editing_mut();
        let Some(input) = editing.inputs.get_mut(&node) else {
            return KeyOutcome::Ignored;
        };
        let buffer = &mut input.buffer;
        let mut copy = None;
        let outcome = match (keysym, letter) {
            (_, Some('a')) if ctrl && !alt => {
                buffer.select_all();
                KeyOutcome::Handled
            }
            (_, Some('c')) if ctrl && !alt => {
                copy = (!password && buffer.has_selection())
                    .then(|| buffer.selected_text().to_owned());
                KeyOutcome::Handled
            }
            (keysym::INSERT | keysym::KP_INSERT, _) if ctrl && !shift => {
                copy = (!password && buffer.has_selection())
                    .then(|| buffer.selected_text().to_owned());
                KeyOutcome::Handled
            }
            (_, Some('x')) if ctrl && !alt => {
                if !password && !read_only && buffer.has_selection() {
                    copy = Some(buffer.selected_text().to_owned());
                    edited = buffer.erase_selection();
                }
                KeyOutcome::Handled
            }
            (keysym::DELETE | keysym::KP_DELETE, _) if shift && !ctrl && buffer.has_selection() => {
                if !password && !read_only {
                    copy = Some(buffer.selected_text().to_owned());
                    edited = buffer.erase_selection();
                }
                KeyOutcome::Handled
            }
            // Shift+Delete is cut, and with nothing selected there is nothing
            // to cut: rather than quietly delete a character, the key goes to
            // `on_key_pressed`, where a launcher may bind it ("forget this
            // entry"). Plain Delete still deletes.
            (keysym::DELETE | keysym::KP_DELETE, _) if shift && !ctrl && !alt => {
                KeyOutcome::Ignored
            }
            (_, Some('v')) if ctrl && !alt => {
                if !read_only && let Some(pasted) = &clipboard {
                    edited = buffer.insert(pasted);
                }
                KeyOutcome::Handled
            }
            (keysym::INSERT | keysym::KP_INSERT, _) if shift && !ctrl => {
                if !read_only && let Some(pasted) = &clipboard {
                    edited = buffer.insert(pasted);
                }
                KeyOutcome::Handled
            }
            (_, Some('z')) if ctrl && !alt => {
                if !read_only {
                    edited = if shift { buffer.redo() } else { buffer.undo() };
                }
                KeyOutcome::Handled
            }
            (_, Some('y')) if ctrl && !alt => {
                if !read_only {
                    edited = buffer.redo();
                }
                KeyOutcome::Handled
            }
            (keysym::BACKSPACE, _) if !alt => {
                if !read_only {
                    edited = buffer.backspace(ctrl);
                }
                KeyOutcome::Handled
            }
            (keysym::DELETE | keysym::KP_DELETE, _) if !alt => {
                if !read_only {
                    edited = buffer.delete(ctrl);
                }
                KeyOutcome::Handled
            }
            (keysym::LEFT | keysym::KP_LEFT, _) if !alt => {
                buffer.move_left(ctrl, shift);
                KeyOutcome::Handled
            }
            (keysym::RIGHT | keysym::KP_RIGHT, _) if !alt => {
                buffer.move_right(ctrl, shift);
                KeyOutcome::Handled
            }
            (keysym::HOME | keysym::KP_HOME, _) if !alt => {
                if ctrl || !multiline {
                    buffer.move_start(shift);
                } else {
                    let cursor = shown.to_display(buffer.text(), buffer.cursor());
                    let (start, _) = map.line_bounds(cursor);
                    buffer.set_cursor(shown.to_model(buffer.text(), start), shift);
                }
                KeyOutcome::Handled
            }
            (keysym::END | keysym::KP_END, _) if !alt => {
                if ctrl || !multiline {
                    buffer.move_end(shift);
                } else {
                    let cursor = shown.to_display(buffer.text(), buffer.cursor());
                    let (_, end) = map.line_bounds(cursor);
                    buffer.set_cursor(shown.to_model(buffer.text(), end), shift);
                }
                KeyOutcome::Handled
            }
            (
                keysym::UP
                | keysym::KP_UP
                | keysym::DOWN
                | keysym::KP_DOWN
                | keysym::PAGE_UP
                | keysym::KP_PAGE_UP
                | keysym::PAGE_DOWN
                | keysym::KP_PAGE_DOWN,
                _,
            ) if multiline && !alt && !ctrl => {
                let lines = match keysym {
                    keysym::UP | keysym::KP_UP => -1,
                    keysym::DOWN | keysym::KP_DOWN => 1,
                    keysym::PAGE_UP | keysym::KP_PAGE_UP => -page,
                    _ => page,
                };
                let cursor = shown.to_display(buffer.text(), buffer.cursor());
                let goal = *input.goal_x.get_or_insert_with(|| map.caret(cursor).x);
                let target = match map.vertical(cursor, lines, goal) {
                    Some(target) => {
                        vertical = true;
                        shown.to_model(buffer.text(), target)
                    }
                    // Off the top is the start, off the bottom the end, and
                    // the column is given up there.
                    None if lines < 0 => 0,
                    None => buffer.text().len(),
                };
                buffer.set_cursor(target, shift);
                KeyOutcome::Handled
            }
            (keysym::RETURN | keysym::KP_ENTER, _) if multiline && !ctrl && !alt => {
                if !read_only {
                    edited = buffer.insert("\n");
                }
                KeyOutcome::Handled
            }
            (keysym::RETURN | keysym::KP_ENTER, _) => {
                let text = buffer.text().to_owned();
                editing
                    .events
                    .push((node, UiEvent::Accepted, vec![IpcValue::String(text)]));
                KeyOutcome::Handled
            }
            (keysym::ESCAPE, _) => {
                editing.events.push((node, UiEvent::Escape, Vec::new()));
                KeyOutcome::Handled
            }
            _ => match text.filter(|text| !text.is_empty()) {
                // Text a key produced, unless a shortcut modifier was held with
                // it: control and a letter is a command, not a letter.
                Some(text) if !ctrl && !alt && !modifiers.logo => {
                    let printable = text.chars().any(|c| !c.is_control());
                    if printable && !read_only {
                        edited = buffer.insert(text);
                    }
                    if printable {
                        KeyOutcome::Handled
                    } else {
                        KeyOutcome::Ignored
                    }
                }
                _ => KeyOutcome::Ignored,
            },
        };
        if let Some(copied) = copy {
            state.copy(copied);
        }
        outcome
    };
    if outcome == KeyOutcome::Handled {
        if !vertical && let Some(input) = state.editing_mut().inputs.get_mut(&node) {
            input.goal_x = None;
        }
        push(state, node, edited);
    }
    outcome
}
