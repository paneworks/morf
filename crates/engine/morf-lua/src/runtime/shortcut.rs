//! Keyboard shortcuts on nodes.
//!
//! `shortcuts = { ["ctrl+b"] = fn, ["ctrl+k ctrl+s"] = fn }` on any node.
//! A key goes to the shortcuts before it goes to the node with focus: first
//! those on the focused node and its ancestors, nearest first, then those of
//! any shown node on the surface whose table says `scope = "surface"`. A
//! shortcut's function is called with its sequence; returning false passes
//! the key on as though it had not matched.
//!
//! A sequence is chords separated by spaces; a chord is modifiers and a key
//! joined by `+` (`ctrl`, `shift`, `alt`, `super`, and any key name
//! `morf.keys` knows -- `back` and `forward` are a mouse's side buttons too).
//! A key that begins a longer sequence is held until the next key either
//! finishes it or does not, and then goes on as usual. Plain keys (no Ctrl,
//! Alt or Super) never reach shortcuts while a text input or terminal has
//! focus: they are typing. Nor do a field's editing chords (Ctrl+A, C, X,
//! V, Z, Y, and Ctrl with a key that moves or deletes), nor anything but
//! Super-chords while a terminal has focus: its program's keys are its own.
//!
//! A chord that is a modifier alone -- `"alt"`, `"super"`, `"ctrl"`,
//! `"shift"` -- is a tap: the key pressed and let go with nothing else in
//! between (no other key, no click). Alt tapped is how a menu bar is
//! reached; Alt held with a letter is still an Alt chord.

use std::time::{Duration, Instant};

use morf_scene::{Element, NodeHandle};

use crate::IpcValue;
use crate::reactive_execute::execute_ipc_handler;
use crate::runtime::handler::Handler;
use crate::text_inputs::KeyModifiers;
use crate::types::LogLevel;

/// How long the first chords of a sequence wait for the next.
pub(crate) const SEQUENCE_TIMEOUT: Duration = Duration::from_millis(1500);

/// One chord: the modifiers held and the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Chord {
    pub(crate) ctrl: bool,
    pub(crate) shift: bool,
    pub(crate) alt: bool,
    pub(crate) logo: bool,
    pub(crate) key: u32,
}

/// A letter's keysym in lower case: Shift+K arrives as `K`, and
/// `"ctrl+shift+k"` should match it.
fn fold(keysym: u32) -> u32 {
    if (0x41..=0x5a).contains(&keysym) {
        keysym + 0x20
    } else {
        keysym
    }
}

impl Chord {
    /// The chord a press makes.
    pub(crate) fn pressed(keysym: u32, modifiers: KeyModifiers) -> Self {
        Self {
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            logo: modifiers.logo,
            key: fold(keysym),
        }
    }

    /// Whether the chord has no Ctrl, Alt or Super: a key that types.
    pub(crate) fn plain(self) -> bool {
        !self.ctrl && !self.alt && !self.logo
    }

    /// Parses `"ctrl+shift+k"`.
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let mut chord = Self {
            ctrl: false,
            shift: false,
            alt: false,
            logo: false,
            key: 0,
        };
        let parts: Vec<&str> = if text.ends_with("++") || text == "+" {
            let mut parts: Vec<&str> = text[..text.len() - 1]
                .split('+')
                .filter(|p| !p.is_empty())
                .collect();
            parts.push("+");
            parts
        } else {
            text.split('+').collect()
        };
        let (key, modifiers) = parts
            .split_last()
            .ok_or_else(|| format!("`{text}` names no key"))?;
        // A modifier alone is its tap.
        if modifiers.is_empty()
            && let Some(tap) = tap_key(&key.to_ascii_lowercase())
        {
            chord.key = tap;
            return Ok(chord);
        }
        for modifier in modifiers {
            match modifier.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => chord.ctrl = true,
                "shift" => chord.shift = true,
                "alt" => chord.alt = true,
                "super" | "logo" | "meta" | "mod4" => chord.logo = true,
                other => {
                    return Err(format!(
                        "`{other}` in `{text}` is not ctrl, shift, alt or super"
                    ));
                }
            }
        }
        let keysym = crate::keys::keysym(key)
            .or_else(|| crate::keys::keysym(&key.to_lowercase()))
            .ok_or_else(|| format!("`{key}` in `{text}` is not a key name"))?;
        chord.key = fold(keysym);
        Ok(chord)
    }
}

/// Parses a sequence: chords separated by spaces.
pub(crate) fn parse_sequence(text: &str) -> Result<Vec<Chord>, String> {
    let chords = text
        .split_whitespace()
        .map(Chord::parse)
        .collect::<Result<Vec<_>, _>>()?;
    if chords.is_empty() {
        return Err("a shortcut needs at least one key".to_owned());
    }
    Ok(chords)
}

/// One node's shortcuts.
pub(crate) struct NodeShortcuts {
    /// Whether they hold anywhere on the surface rather than only around
    /// focus.
    pub(crate) surface: bool,
    pub(crate) entries: Vec<(String, Vec<Chord>, Handler)>,
}

/// The first chords of a sequence, held on one surface.
#[derive(Default)]
pub(crate) struct Pending {
    pub(crate) root: Option<NodeHandle>,
    pub(crate) chords: Vec<Chord>,
    pub(crate) since: Option<Instant>,
}

/// Whether a node is a place keys type into.
pub(crate) fn types_keys(element: Option<Element>) -> bool {
    matches!(element, Some(Element::TextInput | Element::Terminal))
}

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

/// Whether a keysym is only a modifier going down.
/// The keysym a modifier's tap is known by: the left one of a pair.
fn tap_key(name: &str) -> Option<u32> {
    Some(match name {
        "alt" => 0xffe9,
        "super" | "logo" | "meta" => 0xffeb,
        "ctrl" | "control" => 0xffe3,
        "shift" => 0xffe1,
        _ => return None,
    })
}

/// A modifier's keysym folded to its left one: Alt_R taps as Alt.
fn tap_of(keysym: u32) -> Option<u32> {
    Some(match keysym {
        0xffe9 | 0xffea | 0xfe03 => 0xffe9,
        0xffeb | 0xffec | 0xffe7 | 0xffe8 => 0xffeb,
        0xffe3 | 0xffe4 => 0xffe3,
        0xffe1 | 0xffe2 => 0xffe1,
        _ => return None,
    })
}

fn modifier_only(keysym: u32) -> bool {
    (0xffe1..=0xffee).contains(&keysym) || keysym == 0xfe03
}

/// Whether a chord is one a text field edits with: Ctrl with A, C, X, V,
/// Z or Y, or with a key that moves or deletes.
fn editing_chord(chord: Chord) -> bool {
    const EDITING: &[u32] = &[
        0xff08, 0xffff, 0xff9f, 0xff63, 0xff9e, 0xff51, 0xff52, 0xff53, 0xff54, 0xff50, 0xff57,
        0xff0d, 0xff8d,
    ];
    chord.ctrl
        && !chord.alt
        && !chord.logo
        && (matches!(chord.key, 0x61 | 0x63 | 0x76 | 0x78 | 0x79 | 0x7a)
            || EDITING.contains(&chord.key))
}

/// Whether a plain key still means something besides typing: a function
/// key or a media key.
fn beyond_typing(keysym: u32) -> bool {
    (0xffbe..=0xffe0).contains(&keysym) || (0x1008_ff00..=0x1008_ffff).contains(&keysym)
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
            (Some(a), Some(b)) if a == b => {
                let chord = Chord {
                    ctrl: false,
                    shift: false,
                    alt: false,
                    logo: false,
                    key: a,
                };
                self.run_sequence(root, target, &[chord])
            }
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
        let typing = types_keys(element);
        // A field keeps its editing chords and a terminal its program's
        // keys: only Super reaches past a terminal.
        let kept = (element == Some(Element::TextInput) && editing_chord(chord))
            || (element == Some(Element::Terminal) && !chord.logo);
        if kept || (typing && chord.plain() && !beyond_typing(chord.key)) {
            self.reactive.borrow_mut().shortcut_pending = Pending::default();
            return false;
        }
        let held = {
            let mut state = self.reactive.borrow_mut();
            let pending = std::mem::take(&mut state.shortcut_pending);
            let fresh = pending.root == Some(root)
                && pending
                    .since
                    .is_some_and(|since| since.elapsed() < SEQUENCE_TIMEOUT);
            if fresh { pending.chords } else { Vec::new() }
        };
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
            let mut owners = Vec::new();
            let mut current = target.or(Some(root));
            while let Some(node) = current {
                if state.shortcuts.contains_key(&node) && state.scene.can_hold_focus(node) {
                    owners.push(node);
                }
                current = state.scene.parent(node).ok().flatten();
            }
            // In tree order, so the nearer to the top of the surface wins.
            let surface = state.scene.focus_nodes(root, |node| {
                state.shortcuts.get(&node).is_some_and(|s| s.surface) && !owners.contains(&node)
            });
            owners.extend(surface);
            let mut exact = Vec::new();
            let mut prefix = false;
            for node in owners {
                for (name, chords, closure) in &state.shortcuts[&node].entries {
                    if chords.as_slice() == sequence {
                        exact.push((node, name.clone(), closure.clone()));
                    } else if chords.len() > sequence.len() && chords.starts_with(sequence) {
                        prefix = true;
                    }
                }
            }
            (exact, prefix)
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
            let mut state = self.reactive.borrow_mut();
            state.shortcut_pending = Pending {
                root: Some(root),
                chords: sequence.to_vec(),
                since: Some(Instant::now()),
            };
            return true;
        }
        false
    }
}
