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
//! Everything is a function of an [`EditHost`] -- the scene, this state, and
//! a way to write a property -- rather than of the loop, so the same editing
//! serves a key from the compositor and a `:insert()` from inside a handler.
//! Callbacks the edits owe are queued rather than run: a handler cannot be
//! called back into mid-call, and a key should not run the configuration
//! while the state it is editing is still borrowed.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::Instant;

use morf_layout::{Geometry, InputDisplay};
use morf_scene::{Element, NodeHandle, Scene, Value};
use morf_text::{CaretMap, EditBuffer};
use morf_value::IpcValue;

use crate::events::{KeyModifiers, UiEvent};
use crate::requests::TextInputRequest;

mod frame;
mod keys;
mod methods;
mod pointer;

pub use frame::{blink, next_blink, observe_shaped};
pub use keys::{KeyOutcome, key};
use methods::{caret_map, display};
pub use methods::{history, insert, select, select_all, selected_text, tracked};
use pointer::tell_input_method;
pub use pointer::{drag, input_method_commit, press, reconcile_focus, release, set_focus};

/// How many rounds of callbacks one event may set off before the rest are
/// dropped: a handler that edits its own field from `on_text_changed` would
/// otherwise go round for ever.
pub const CALLBACK_ROUNDS: usize = 16;

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
pub struct InputState {
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

/// Every live text input, which has the keyboard, and the callbacks they
/// owe.
#[derive(Default)]
pub struct Editing {
    /// Every live text input's editing state.
    pub inputs: HashMap<NodeHandle, InputState>,
    /// When each text input was made, so ties between them are settled the
    /// same way every time.
    pub order: HashMap<NodeHandle, u64>,
    pub last: u64,
    /// The text input that has the keyboard, if one does.
    pub focused: Option<NodeHandle>,
    /// Callbacks owed, run once whatever made them is done.
    pub events: Vec<(NodeHandle, UiEvent, Vec<IpcValue>)>,
    /// Whether those callbacks are being run, so running one does not start
    /// running them again from inside itself.
    pub draining: bool,
}

impl Editing {
    /// Forgets the fields among `removed`, and what they owed.
    pub fn forget(&mut self, removed: &HashSet<NodeHandle>) {
        if self.focused.is_some_and(|node| removed.contains(&node)) {
            self.focused = None;
        }
        self.inputs.retain(|node, _| !removed.contains(node));
        self.order.retain(|node, _| !removed.contains(node));
        self.events.retain(|(node, _, _)| !removed.contains(node));
    }

    /// Starts running the owed callbacks, unless they are being run, a
    /// handler is (`in_handler`), or none are owed: whether to.
    pub fn start_draining(&mut self, in_handler: bool) -> bool {
        if self.draining || in_handler || self.events.is_empty() {
            return false;
        }
        self.draining = true;
        true
    }

    /// Done running them: whether some were still owed after
    /// [`CALLBACK_ROUNDS`] rounds, and were dropped.
    pub fn finish_draining(&mut self) -> bool {
        self.draining = false;
        let dropped = !self.events.is_empty();
        self.events.clear();
        dropped
    }
}

/// What editing needs of whoever holds the scene: to read it, to write a
/// property through whatever bindings follow it, and to reach the
/// clipboard and the compositor's input method.
pub trait EditHost {
    fn scene(&self) -> &Scene;
    fn editing(&self) -> &Editing;
    fn editing_mut(&mut self) -> &mut Editing;
    /// Writes `property` of `node`, as a configuration's write would.
    fn assign(&mut self, node: NodeHandle, property: &str, value: Value) -> Result<(), String>;
    fn warn(&mut self, message: String);
    /// The clipboard's text as last seen, to paste.
    fn clipboard_text(&self) -> Option<String>;
    /// Puts `text` on the clipboard.
    fn copy(&mut self, text: String);
    /// Tells the compositor's input method something.
    fn text_input(&mut self, request: TextInputRequest);
    /// Asks for the compositor's input method to follow the keyboard.
    fn enable_text_input(&mut self);
}

/// Starts tracking a field the configuration just built.
///
/// The caret starts at the end of whatever text it was given, unless the
/// configuration placed it itself.
pub fn register(state: &mut impl EditHost, node: NodeHandle) {
    let Ok(text) = state.scene().string_value(node, "text").map(str::to_owned) else {
        return;
    };
    let mut input = InputState::new(&text);
    configure_buffer(state.scene(), node, &mut input.buffer);
    // Through the field's own rules, now that it knows them: a single line
    // takes no line breaks, and `max_length` holds from the first letter.
    // Built before them, a line given "books\n" kept its break, was shaped
    // as two lines, and drew a caret at its end on the empty line below.
    // `push` writes the text back when this changed it.
    input.buffer.set_text(&text);
    let placed = |name: &str| {
        state
            .scene()
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
    let editing = state.editing_mut();
    editing.inputs.insert(node, input);
    editing.last = editing.last.wrapping_add(1);
    let order = editing.last;
    editing.order.insert(node, order);
    push(state, node, false);
}

/// The field's rules, as its properties now say them.
fn configure_buffer(scene: &Scene, node: NodeHandle, buffer: &mut EditBuffer) {
    buffer.multiline = scene.bool_value(node, "multiline").unwrap_or(false);
    buffer.max_length = scene
        .number(node, "max_length")
        .map_or(0, |value| value.max(0.0) as usize);
}

/// Takes in whatever the configuration wrote since the field last looked.
///
/// A new `text` replaces the buffer and its history; a new caret or
/// selection moves them. Returns false for a node that is no longer a live
/// text input, which is then forgotten.
pub fn pull(state: &mut impl EditHost, node: NodeHandle) -> bool {
    if state.scene().element(node).ok() != Some(Element::TextInput) {
        let editing = state.editing_mut();
        editing.inputs.remove(&node);
        editing.order.remove(&node);
        return false;
    }
    let Some(mut input) = state.editing_mut().inputs.remove(&node) else {
        return false;
    };
    let scene = state.scene();
    configure_buffer(scene, node, &mut input.buffer);
    let text = scene
        .string_value(node, "text")
        .unwrap_or_default()
        .to_owned();
    // Targets, not current values: a behavior on the caret animates the
    // number, and a caret halfway through its animation is not a new place
    // the configuration asked for.
    let number = |name: &str| match scene.target(node, name) {
        Ok(Value::Number(value)) => value.max(0.0) as usize,
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
    state.editing_mut().inputs.insert(node, input);
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
fn push(state: &mut impl EditHost, node: NodeHandle, edited: bool) {
    let Some(input) = state.editing_mut().inputs.get_mut(&node) else {
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
    let focused = state.scene().bool_value(node, "focus").unwrap_or(false);
    if text_changed {
        write(state, node, "text", Value::String(text.clone()));
    }
    write(
        state,
        node,
        "cursor_position",
        Value::Number(now.cursor as f64),
    );
    write(
        state,
        node,
        "selection_start",
        Value::Number(now.start as f64),
    );
    write(state, node, "selection_end", Value::Number(now.end as f64));
    write(state, node, "caret_visible", Value::Bool(true));
    if text_changed && edited {
        state
            .editing_mut()
            .events
            .push((node, UiEvent::TextChanged, vec![IpcValue::String(text)]));
    }
    if focused {
        tell_input_method(state, node);
    }
}

/// Writes one of a field's own properties; a refusal is logged.
fn write(state: &mut impl EditHost, node: NodeHandle, property: &str, value: Value) {
    if let Err(message) = state.assign(node, property, value) {
        state.warn(format!("TextInput.{property}: {message}"));
    }
}

/// The arguments a key handler is called with; `repeat` only for presses.
pub fn key_args(
    keysym: u32,
    text: Option<&str>,
    modifiers: KeyModifiers,
    repeat: Option<bool>,
) -> Vec<IpcValue> {
    let mut args = vec![
        IpcValue::Integer(i64::from(keysym)),
        text.map_or(IpcValue::Nil, |value| IpcValue::String(value.to_owned())),
        IpcValue::String(modifiers.name()),
    ];
    // The key's name comes fifth for a press and a release alike: after
    // whether it repeats, which a release does not say.
    args.push(repeat.map_or(IpcValue::Nil, IpcValue::Boolean));
    args.push(crate::keys::name(keysym).map_or(IpcValue::Nil, IpcValue::String));
    args
}

#[cfg(test)]
mod tests;
