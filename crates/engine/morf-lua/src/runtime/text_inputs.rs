//! Text inputs: the editing that happens between a key and a property.
//!
//! A `ui.TextInput` is a scene node like any other, so everything a
//! configuration can see of it — its text, its caret, its selection, how far
//! it has scrolled — is a property. What the properties cannot hold is the
//! history behind them, the column a caret is trying to keep while it moves
//! up through short lines, or whether the last click was the first half of a
//! double click. That lives here, one [`InputState`] per field, and every
//! change made to it is written straight back to the properties.
//!
//! Everything is a function of the reactive state rather than of the runtime,
//! so the same editing serves a key from the compositor and a `:insert()`
//! from inside a Lua handler. Callbacks the edits owe are queued rather than
//! run: a Lua method cannot call back into Lua mid-call, and a key handler
//! should not run the configuration while the state it is editing is still
//! borrowed.

use std::ops::Range;
use std::time::Instant;

use morf_layout::{Geometry, InputDisplay};
use morf_scene::{Element, NodeHandle, Value as SceneValue};
use morf_text::{CaretMap, EditBuffer};

use crate::{events::UiEvent, scene_bindings::assign_scene_property, state::ReactiveState};
use crate::{surface_types::*, types::LogLevel};

mod frame;
mod keys;
mod pointer;

pub(crate) use frame::{blink, next_blink, observe_shaped};
pub(crate) use keys::{KeyOutcome, key};
use pointer::tell_input_method;
pub(crate) use pointer::{drag, input_method_commit, press, reconcile_focus, release, set_focus};

pub use morf_runtime::events::KeyModifiers;

/// What a drag started by a press extends by.
#[derive(Clone, Debug)]
enum Drag {
    /// Letter by letter, from where the press put the caret.
    Grapheme,
    /// Whole words, never losing the word double-clicked.
    Word(Range<usize>),
    /// Whole lines, never losing the line triple-clicked.
    Line(Range<usize>),
}

#[derive(Clone, Copy, Debug)]
struct Press {
    at: Instant,
    x: f64,
    y: f64,
    count: u8,
}

/// One field's editing state.
#[derive(Clone, Debug)]
pub(crate) struct InputState {
    buffer: EditBuffer,
    /// The caret stops of the text as it was last shaped, and the string
    /// shaped. A stop list for any other string is no use, so it is kept
    /// with the string it describes.
    map: Option<(String, CaretMap)>,
    /// Where the text's first line starts inside the box, from the top: the
    /// room a single centred line leaves above it.
    offset_y: f64,
    /// The box, as the last frame laid it out.
    geometry: Option<Geometry>,
    /// The x a caret moving up and down is keeping to.
    goal_x: Option<f32>,
    /// When the caret last moved, which the blink counts from.
    blink_start: Instant,
    /// What was last written to the node's properties, so a write from the
    /// configuration can be told apart from the field's own.
    written: Written,
    press: Option<Press>,
    drag: Option<Drag>,
    /// The caret rectangle last told to an input method, in surface space.
    ime_rect: Option<(i32, i32, i32, i32)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Written {
    text: String,
    cursor: usize,
    start: usize,
    end: usize,
}

impl InputState {
    fn new(text: &str) -> Self {
        Self {
            buffer: EditBuffer::new(text),
            map: None,
            offset_y: 0.0,
            geometry: None,
            goal_x: None,
            blink_start: Instant::now(),
            written: Written {
                text: text.to_owned(),
                cursor: usize::MAX,
                start: usize::MAX,
                end: usize::MAX,
            },
            press: None,
            drag: None,
            ime_rect: None,
        }
    }
}

/// Starts tracking a field the configuration just built.
///
/// The caret starts at the end of whatever text it was given, unless the
/// configuration placed it itself.
pub(crate) fn register(state: &mut ReactiveState, node: NodeHandle) {
    let Ok(text) = state.scene.string_value(node, "text").map(str::to_owned) else {
        return;
    };
    let mut input = InputState::new(&text);
    configure_buffer(state, node, &mut input.buffer);
    // Through the field's own rules, now that it knows them: a single line
    // takes no line breaks, and `max_length` holds from the first letter.
    // Built before them, a line given "books\n" kept its break, was shaped
    // as two lines, and drew a caret at its end on the empty line below.
    // `push` writes the text back when this changed it.
    input.buffer.set_text(&text);
    let placed = |name: &str| {
        state
            .scene
            .number(node, name)
            .map_or(0, |value| value.max(0.0) as usize)
    };
    let (cursor, start, end) = (
        placed("cursor_position"),
        placed("selection_start"),
        placed("selection_end"),
    );
    if start != end {
        input.buffer.select(start, end);
    } else if cursor != 0 {
        input.buffer.set_cursor(cursor, false);
    }
    state.text_inputs.insert(node, input);
    state.last_text_input = state.last_text_input.wrapping_add(1);
    let order = state.last_text_input;
    state.text_input_order.insert(node, order);
    push(state, node, false);
}

/// The field's rules, as its properties now say them.
fn configure_buffer(state: &ReactiveState, node: NodeHandle, buffer: &mut EditBuffer) {
    buffer.multiline = state.scene.bool_value(node, "multiline").unwrap_or(false);
    buffer.max_length = state
        .scene
        .number(node, "max_length")
        .map_or(0, |value| value.max(0.0) as usize);
}

/// Takes in whatever the configuration wrote since the field last looked.
///
/// A new `text` replaces the buffer and its history; a new caret or
/// selection moves them. Returns false for a node that is no longer a live
/// text input, which is then forgotten.
pub(crate) fn pull(state: &mut ReactiveState, node: NodeHandle) -> bool {
    if state.scene.element(node).ok() != Some(Element::TextInput) {
        state.text_inputs.remove(&node);
        state.text_input_order.remove(&node);
        return false;
    }
    let Some(mut input) = state.text_inputs.remove(&node) else {
        return false;
    };
    configure_buffer(state, node, &mut input.buffer);
    let text = state
        .scene
        .string_value(node, "text")
        .unwrap_or_default()
        .to_owned();
    // Targets, not current values: a behavior on the caret animates the
    // number, and a caret halfway through its animation is not a new place
    // the configuration asked for.
    let number = |name: &str| match state.scene.target(node, name) {
        Ok(SceneValue::Number(value)) => value.max(0.0) as usize,
        _ => 0,
    };
    let (cursor, start, end) = (
        number("cursor_position"),
        number("selection_start"),
        number("selection_end"),
    );
    let mut changed = false;
    if text != input.written.text {
        input.buffer.set_text(&text);
        input.written.text = text;
        changed = true;
    }
    if (start, end) != (input.written.start, input.written.end) {
        if start == end {
            input.buffer.set_cursor(start, false);
        } else {
            input.buffer.select(start, end);
        }
        changed = true;
    } else if cursor != input.written.cursor {
        input.buffer.set_cursor(cursor, false);
        changed = true;
    }
    if changed {
        input.goal_x = None;
    }
    state.text_inputs.insert(node, input);
    if changed {
        push(state, node, false);
    }
    true
}

/// Writes the buffer back to the node's properties.
///
/// `edited` says the text changed because of the field itself — a key, a
/// paste, a method — which is when `on_text_changed` is owed. A write from the
/// configuration is not announced back to it.
fn push(state: &mut ReactiveState, node: NodeHandle, edited: bool) {
    let Some(input) = state.text_inputs.get_mut(&node) else {
        return;
    };
    let text = input.buffer.text().to_owned();
    let selection = input.buffer.selection();
    let now = Written {
        text: text.clone(),
        cursor: input.buffer.cursor(),
        start: selection.start,
        end: selection.end,
    };
    if now == input.written {
        return;
    }
    let text_changed = now.text != input.written.text;
    input.written = now.clone();
    input.blink_start = Instant::now();
    let focused = state.scene.bool_value(node, "focus").unwrap_or(false);
    let write = |state: &mut ReactiveState, property: &str, value: SceneValue| {
        if let Err(message) = assign_scene_property(state, node, property, value) {
            state.log(LogLevel::Warn, format!("TextInput.{property}: {message}"));
        }
    };
    if text_changed {
        write(state, "text", SceneValue::String(text.clone()));
    }
    write(
        state,
        "cursor_position",
        SceneValue::Number(now.cursor as f64),
    );
    write(
        state,
        "selection_start",
        SceneValue::Number(now.start as f64),
    );
    write(state, "selection_end", SceneValue::Number(now.end as f64));
    write(state, "caret_visible", SceneValue::Bool(true));
    if text_changed && edited {
        state
            .input_events
            .push((node, UiEvent::TextChanged, vec![IpcValue::String(text)]));
    }
    if focused {
        tell_input_method(state, node);
    }
}

/// The string the field shapes, and its offsets into the text.
fn display(state: &ReactiveState, node: NodeHandle) -> InputDisplay {
    let text = state.scene.string_value(node, "text").unwrap_or_default();
    let placeholder = state
        .scene
        .string_value(node, "placeholder")
        .unwrap_or_default();
    let mask = state
        .scene
        .bool_value(node, "password")
        .unwrap_or(false)
        .then(|| {
            state
                .scene
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
fn caret_map(state: &ReactiveState, node: NodeHandle, shown: &InputDisplay) -> CaretMap {
    if let Some((text, map)) = state
        .text_inputs
        .get(&node)
        .and_then(|input| input.map.as_ref())
        && *text == shown.text
    {
        return map.clone();
    }
    let size = state
        .scene
        .number(node, "font_size")
        .unwrap_or(16.0)
        .max(1.0);
    let line = state
        .scene
        .current(node, "line_height")
        .ok()
        .and_then(|value| morf_layout::LineHeight::parse(value).ok())
        .unwrap_or_default()
        .pixels(size);
    let text = if shown.placeholder { "" } else { &shown.text };
    CaretMap::uniform(text, (size * 0.6) as f32, line as f32)
}

/// Methods a configuration calls on a field: `:select(a, b)`, `:insert(s)`…
pub(crate) fn select(state: &mut ReactiveState, node: NodeHandle, start: usize, end: usize) {
    if pull(state, node)
        && let Some(input) = state.text_inputs.get_mut(&node)
    {
        input.buffer.select(start, end);
        input.goal_x = None;
        push(state, node, false);
    }
}

/// Selects everything in a field.
pub(crate) fn select_all(state: &mut ReactiveState, node: NodeHandle) {
    if pull(state, node)
        && let Some(input) = state.text_inputs.get_mut(&node)
    {
        input.buffer.select_all();
        push(state, node, false);
    }
}

/// Types into a field from the configuration, as if from the keyboard.
pub(crate) fn insert(state: &mut ReactiveState, node: NodeHandle, text: &str) -> bool {
    if !pull(state, node) {
        return false;
    }
    let Some(input) = state.text_inputs.get_mut(&node) else {
        return false;
    };
    let edited = input.buffer.insert(text);
    push(state, node, edited);
    edited
}

/// The field's selected text.
pub(crate) fn selected_text(state: &mut ReactiveState, node: NodeHandle) -> String {
    if !pull(state, node) {
        return String::new();
    }
    state
        .text_inputs
        .get(&node)
        .map(|input| input.buffer.selected_text().to_owned())
        .unwrap_or_default()
}

/// Undoes or redoes the field's last edit.
pub(crate) fn history(state: &mut ReactiveState, node: NodeHandle, redo: bool) -> bool {
    if !pull(state, node) {
        return false;
    }
    let Some(input) = state.text_inputs.get_mut(&node) else {
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

/// The fields a layout pass placed, with the box each was given.
pub(crate) fn tracked(state: &ReactiveState) -> Vec<NodeHandle> {
    let mut nodes = state.text_inputs.keys().copied().collect::<Vec<_>>();
    nodes.sort_by_key(|node| state.text_input_order.get(node).copied());
    nodes
}
