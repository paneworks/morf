//! Keyboard shortcuts on nodes, as a configuration declares them
//! (`shortcuts = { ["ctrl+b"] = fn }`) and as keys run them. The chords,
//! sequences, taps and matching are morf-runtime's.

use morf_scene::NodeHandle;

use crate::IpcValue;
use crate::reactive_execute::execute_ipc_handler;
use crate::text_inputs::KeyModifiers;
use crate::types::LogLevel;

pub(crate) use morf_runtime::shortcuts::*;

/// Reads a `shortcuts` table: `scope` and sequence = function pairs.
pub(crate) fn read_table<'gc>(
    ctx: luna::Context<'gc>,
    value: luna::Value<'gc>,
) -> Result<Option<NodeShortcuts>, String> {
    let table = match value {
        luna::Value::Nil | luna::Value::Boolean(false) => return Ok(None),
        luna::Value::Table(table) => table,
        _ => return Err("shortcuts must be a table of key = function".to_owned()),
    };
    let mut shortcuts = NodeShortcuts {
        surface: false,
        entries: Vec::new(),
    };
    for (key, value) in table.iter(ctx) {
        let luna::Value::String(key) = key else {
            return Err("a shortcut's key must be a string such as \"ctrl+b\"".to_owned());
        };
        let key = key
            .to_str()
            .map_err(|_| "a shortcut's key must be text".to_owned())?
            .to_owned();
        if key == "scope" {
            shortcuts.surface = match value {
                luna::Value::String(scope) if scope.as_bytes() == b"surface" => true,
                luna::Value::String(scope) if scope.as_bytes() == b"focus" => false,
                _ => return Err("shortcuts.scope must be \"focus\" or \"surface\"".to_owned()),
            };
            continue;
        }
        let luna::Value::Function(luna::Function::Closure(closure)) = value else {
            return Err(format!("shortcut `{key}` must be a function"));
        };
        let chords = parse_sequence(&key)?;
        shortcuts.entries.push((
            key,
            chords,
            crate::vm::handler_store::register(ctx.stash(closure)),
        ));
    }
    // In a fixed order, so which of two equal sequences wins does not depend
    // on how the table hashed.
    shortcuts.entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Some(shortcuts))
}

impl crate::Runtime {
    /// Runs the shortcut a key press makes on the surface whose tree is
    /// `root`, with `target` the node keys go to there. Returns whether the
    /// key was taken -- by a shortcut, or as the start of a sequence.
    /// Follows the keys for a modifier's tap: a modifier pressed on its own
    /// starts one, any other key or a pointer press breaks it, and its own
    /// release finishes it -- then a shortcut naming it (`"alt"`) runs.
    /// Returns whether one did.
    pub fn note_key_for_tap(
        &mut self,
        root: NodeHandle,
        target: Option<NodeHandle>,
        keysym: u32,
        pressed: bool,
    ) -> bool {
        let tap = tap_of(keysym);
        if pressed {
            self.reactive.borrow_mut().modifier_tap = tap;
            return false;
        }
        let started = self.reactive.borrow_mut().modifier_tap.take();
        match (started, tap) {
            (Some(a), Some(b)) if a == b => self.run_sequence(root, target, &[tap_chord(a)]),
            _ => false,
        }
    }

    /// A pointer press while a modifier is down is not a tap of it (Alt and
    /// a drag moves a window).
    pub fn break_modifier_tap(&mut self) {
        self.reactive.borrow_mut().modifier_tap = None;
    }

    pub fn dispatch_shortcut(
        &mut self,
        root: NodeHandle,
        target: Option<NodeHandle>,
        keysym: u32,
        modifiers: KeyModifiers,
    ) -> bool {
        if modifier_only(keysym) {
            return false;
        }
        let chord = Chord::pressed(keysym, modifiers);
        let element = target.and_then(|node| self.reactive.borrow().scene.element(node).ok());
        if !reaches_shortcuts(element, chord) {
            self.reactive.borrow_mut().shortcut_pending = Pending::default();
            return false;
        }
        let held = std::mem::take(&mut self.reactive.borrow_mut().shortcut_pending).held_for(root);
        let had_prefix = !held.is_empty();
        let mut sequence = held;
        sequence.push(chord);
        if self.run_sequence(root, target, &sequence) {
            return true;
        }
        // A sequence the key broke: the key goes on alone.
        had_prefix && self.run_sequence(root, target, &[chord])
    }

    fn run_sequence(
        &mut self,
        root: NodeHandle,
        target: Option<NodeHandle>,
        sequence: &[Chord],
    ) -> bool {
        let (exact, prefix) = {
            let state = self.reactive.borrow();
            if state.shortcuts.is_empty() {
                return false;
            }
            matching(&state.scene, &state.shortcuts, root, target, sequence)
        };
        for (node, name, closure) in exact {
            let args = [IpcValue::String(name.clone())];
            match self.run_handler(|ctx, limits| execute_ipc_handler(ctx, &closure, &args, limits))
            {
                Ok(values) if values.first() == Some(&IpcValue::Boolean(false)) => continue,
                Ok(_) => return true,
                Err(message) => {
                    self.reactive.borrow_mut().log(
                        LogLevel::Warn,
                        format!("{node:?} shortcut `{name}`: {message}"),
                    );
                    return true;
                }
            }
        }
        if prefix {
            self.reactive.borrow_mut().shortcut_pending = Pending::holding(root, sequence);
            return true;
        }
        false
    }
}
