//! What a field shows and where its caret can stop, and the methods a
//! configuration calls on it: `:select(a, b)`, `:insert(s)`, `:undo()`…

use super::*;

/// The string the field shapes, and its offsets into the text.
pub(super) fn display(scene: &Scene, node: NodeHandle) -> InputDisplay {
    let text = scene.string_value(node, "text").unwrap_or_default();
    let placeholder = scene.string_value(node, "placeholder").unwrap_or_default();
    let mask = scene
        .bool_value(node, "password")
        .unwrap_or(false)
        .then(|| {
            scene
                .string_value(node, "password_char")
                .ok()
                .and_then(|mask| mask.chars().next())
                .unwrap_or('•')
        });
    InputDisplay::new(text, placeholder, mask)
}

/// The caret stops the field's text has now.
///
/// The last frame's, when it shaped this very string. Between an edit and
/// the frame that draws it — and in a runtime with no renderer at all —
/// there is only the string, so a stand-in grid of even advances answers
/// until the real one arrives.
pub(super) fn caret_map(state: &impl EditHost, node: NodeHandle, shown: &InputDisplay) -> CaretMap {
    if let Some((text, map)) = state
        .editing()
        .inputs
        .get(&node)
        .and_then(|input| input.map.as_ref())
        && *text == shown.text
    {
        return map.clone();
    }
    let scene = state.scene();
    let size = scene.number(node, "font_size").unwrap_or(16.0).max(1.0);
    let line = scene
        .current(node, "line_height")
        .ok()
        .and_then(|value| morf_layout::LineHeight::parse(value).ok())
        .unwrap_or_default()
        .pixels(size);
    let text = if shown.placeholder { "" } else { &shown.text };
    CaretMap::uniform(text, (size * 0.6) as f32, line as f32)
}

/// Selects from `start` to `end` in a field.
pub fn select(state: &mut impl EditHost, node: NodeHandle, start: usize, end: usize) {
    if pull(state, node)
        && let Some(input) = state.editing_mut().inputs.get_mut(&node)
    {
        input.buffer.select(start, end);
        input.goal_x = None;
        push(state, node, false);
    }
}

/// Selects everything in a field.
pub fn select_all(state: &mut impl EditHost, node: NodeHandle) {
    if pull(state, node)
        && let Some(input) = state.editing_mut().inputs.get_mut(&node)
    {
        input.buffer.select_all();
        push(state, node, false);
    }
}

/// Types into a field from the configuration, as if from the keyboard.
pub fn insert(state: &mut impl EditHost, node: NodeHandle, text: &str) -> bool {
    if !pull(state, node) {
        return false;
    }
    let Some(input) = state.editing_mut().inputs.get_mut(&node) else {
        return false;
    };
    let edited = input.buffer.insert(text);
    push(state, node, edited);
    edited
}

/// The field's selected text.
pub fn selected_text(state: &mut impl EditHost, node: NodeHandle) -> String {
    if !pull(state, node) {
        return String::new();
    }
    state
        .editing()
        .inputs
        .get(&node)
        .map(|input| input.buffer.selected_text().to_owned())
        .unwrap_or_default()
}

/// Undoes or redoes the field's last edit.
pub fn history(state: &mut impl EditHost, node: NodeHandle, redo: bool) -> bool {
    if !pull(state, node) {
        return false;
    }
    let Some(input) = state.editing_mut().inputs.get_mut(&node) else {
        return false;
    };
    let edited = if redo {
        input.buffer.redo()
    } else {
        input.buffer.undo()
    };
    push(state, node, edited);
    edited
}

/// The fields a layout pass placed, in the order they were made.
pub fn tracked(editing: &Editing) -> Vec<NodeHandle> {
    let mut nodes = editing.inputs.keys().copied().collect::<Vec<_>>();
    nodes.sort_by_key(|node| editing.order.get(node).copied());
    nodes
}
