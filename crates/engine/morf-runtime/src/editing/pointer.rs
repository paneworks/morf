//! The pointer on a field, and which field has the keyboard.

use std::time::Duration;

use super::*;

/// How long between two presses still makes them one double click.
const MULTI_CLICK: Duration = Duration::from_millis(400);
/// How far apart two presses may land and still be one double click.
const MULTI_CLICK_SLOP: f64 = 4.0;

/// Turns a point in the node into an offset in its text.
fn offset_at(state: &impl EditHost, node: NodeHandle, local: (f64, f64)) -> usize {
    let shown = display(state.scene(), node);
    let map = caret_map(state, node, &shown);
    let offset_y = state
        .editing()
        .inputs
        .get(&node)
        .map_or(0.0, |input| input.offset_y);
    let x = local.0 + state.scene().number(node, "scroll_x").unwrap_or(0.0);
    let y = local.1 + state.scene().number(node, "scroll_y").unwrap_or(0.0) - offset_y;
    let text = state.scene().string_value(node, "text").unwrap_or_default();
    shown.to_model(text, map.index_at(x as f32, y as f32))
}

/// A press on the field: focus, and a caret where it landed — or a word for
/// a double click, a line for a triple.
pub fn press(state: &mut impl EditHost, node: NodeHandle, local: (f64, f64)) {
    if !pull(state, node) {
        return;
    }
    set_focus(state, node, true);
    let offset = offset_at(state, node, local);
    let now = Instant::now();
    let Some(input) = state.editing_mut().inputs.get_mut(&node) else {
        return;
    };
    let count = match input.press {
        Some(last)
            if now.duration_since(last.at) <= MULTI_CLICK
                && (last.x - local.0).hypot(last.y - local.1) <= MULTI_CLICK_SLOP =>
        {
            last.count % 3 + 1
        }
        _ => 1,
    };
    input.press = Some(Press {
        at: now,
        x: local.0,
        y: local.1,
        count,
    });
    input.goal_x = None;
    let buffer = &mut input.buffer;
    input.drag = Some(match count {
        1 => {
            buffer.set_cursor(offset, false);
            Drag::Grapheme
        }
        2 => {
            let word = buffer.word_at(offset);
            buffer.select(word.start, word.end);
            Drag::Word(word)
        }
        _ => {
            let line = if buffer.multiline {
                buffer.line_at(offset)
            } else {
                0..buffer.text().len()
            };
            buffer.select(line.start, line.end);
            Drag::Line(line)
        }
    });
    push(state, node, false);
}

/// The pointer moved while a press on the field is held: the selection
/// follows it, a letter, a word or a line at a time.
pub fn drag(state: &mut impl EditHost, node: NodeHandle, local: (f64, f64)) -> bool {
    if state
        .editing()
        .inputs
        .get(&node)
        .is_none_or(|input| input.drag.is_none())
        || !pull(state, node)
    {
        return false;
    }
    let offset = offset_at(state, node, local);
    let Some(input) = state.editing_mut().inputs.get_mut(&node) else {
        return false;
    };
    let buffer = &mut input.buffer;
    match input.drag.clone() {
        Some(Drag::Grapheme) => buffer.set_cursor(offset, true),
        Some(Drag::Word(held)) | Some(Drag::Line(held)) => {
            let reach = if matches!(input.drag, Some(Drag::Word(_))) {
                buffer.word_at(offset)
            } else if buffer.multiline {
                buffer.line_at(offset)
            } else {
                0..buffer.text().len()
            };
            if reach.start < held.start {
                buffer.select(held.end, reach.start);
            } else {
                buffer.select(held.start, reach.end.max(held.end));
            }
        }
        None => {}
    }
    push(state, node, false);
    true
}

/// The press on the field ended.
pub fn release(state: &mut impl EditHost, node: NodeHandle) {
    if let Some(input) = state.editing_mut().inputs.get_mut(&node) {
        input.drag = None;
    }
}

/// Gives the field the keyboard, or takes it away.
///
/// One field has it at a time, so focusing one takes it from whichever had
/// it. The `on_focus_changed` callbacks this owes are queued with the rest.
pub fn set_focus(state: &mut impl EditHost, node: NodeHandle, focused: bool) {
    if state.scene().element(node).ok() != Some(Element::TextInput) {
        return;
    }
    if let Err(message) = state.assign(node, "focus", Value::Bool(focused)) {
        state.warn(format!("TextInput.focus: {message}"));
    }
    reconcile_focus(state);
}

/// Settles which field has the keyboard after anything may have written
/// `focus`.
///
/// The field that most recently claimed it keeps it; any other that still
/// says `focus = true` is told it has lost it.
pub fn reconcile_focus(state: &mut impl EditHost) {
    let current = state
        .editing()
        .focused
        .filter(|node| state.scene().element(*node).ok() == Some(Element::TextInput));
    let mut claimed = state
        .editing()
        .inputs
        .keys()
        .copied()
        .filter(|node| Some(*node) != current)
        .filter(|node| state.scene().bool_value(*node, "focus").unwrap_or(false))
        .collect::<Vec<_>>();
    // In creation order, so which of two claims wins does not depend on how a
    // hash map happens to be laid out.
    claimed.sort_by_key(|node| state.editing().order.get(node).copied());
    let still =
        current.is_some_and(|node| state.scene().bool_value(node, "focus").unwrap_or(false));
    let next = claimed.last().copied().or(current.filter(|_| still));
    for node in claimed.iter().chain(current.iter()) {
        if Some(*node) != next && state.scene().bool_value(*node, "focus").unwrap_or(false) {
            let _ = state.assign(*node, "focus", Value::Bool(false));
        }
    }
    if next == state.editing().focused {
        return;
    }
    if let Some(old) = state.editing().focused {
        state.editing_mut().events.push((
            old,
            UiEvent::FocusChanged,
            vec![IpcValue::Boolean(false)],
        ));
        if let Some(input) = state.editing_mut().inputs.get_mut(&old) {
            input.drag = None;
            input.ime_rect = None;
        }
    }
    state.editing_mut().focused = next;
    match next {
        Some(new) => {
            state.editing_mut().events.push((
                new,
                UiEvent::FocusChanged,
                vec![IpcValue::Boolean(true)],
            ));
            if let Some(input) = state.editing_mut().inputs.get_mut(&new) {
                input.blink_start = Instant::now();
            }
            let _ = state.assign(new, "caret_visible", Value::Bool(true));
            // The compositor's input method follows the keyboard into the
            // field: typing through it arrives as text for this one.
            state.enable_text_input();
            let password = state.scene().bool_value(new, "password").unwrap_or(false);
            let multiline = state.scene().bool_value(new, "multiline").unwrap_or(false);
            // text-input-v3: hidden text and sensitive data for a password,
            // with the password purpose; a multi-line hint otherwise.
            let (hints, purpose) = if password {
                (0x40 | 0x80, 8)
            } else if multiline {
                (0x200, 0)
            } else {
                (0, 0)
            };
            state.text_input(TextInputRequest::ContentType { hints, purpose });
            tell_input_method(state, new);
        }
        None => state.text_input(TextInputRequest::Disable),
    }
}

/// Tells the input method what surrounds the caret of the focused field.
pub(super) fn tell_input_method(state: &mut impl EditHost, node: NodeHandle) {
    let Some(input) = state.editing().inputs.get(&node) else {
        return;
    };
    // A password's letters are not the input method's business.
    let (text, cursor, anchor) = if state.scene().bool_value(node, "password").unwrap_or(false) {
        (String::new(), 0, 0)
    } else {
        (
            input.buffer.text().to_owned(),
            input.buffer.cursor() as i32,
            input.buffer.anchor() as i32,
        )
    };
    // The protocol caps surrounding text at four kilobytes.
    if text.len() > 4000 {
        return;
    }
    state.text_input(TextInputRequest::Surrounding {
        text,
        cursor,
        anchor,
    });
}

/// An input method's committed batch, applied to the focused field: the
/// text either side of the caret it asked deleted, and what it committed.
pub fn input_method_commit(
    state: &mut impl EditHost,
    commit: Option<&str>,
    before: u32,
    after: u32,
) -> bool {
    let Some(node) = state.editing().focused else {
        return false;
    };
    if !pull(state, node) || state.scene().bool_value(node, "read_only").unwrap_or(false) {
        return false;
    }
    let Some(input) = state.editing_mut().inputs.get_mut(&node) else {
        return false;
    };
    let buffer = &mut input.buffer;
    let selection = buffer.selection();
    let start = selection.start.saturating_sub(before as usize);
    let end = (selection.end + after as usize).min(buffer.text().len());
    let edited = if before > 0 || after > 0 {
        buffer.replace(start..end, commit.unwrap_or_default())
    } else if let Some(commit) = commit {
        buffer.insert(commit)
    } else {
        false
    };
    push(state, node, edited);
    edited
}
