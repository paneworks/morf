//! Keyboard shortcuts on nodes: chords, sequences, taps, and which node's
//! shortcut a key runs.
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

use std::collections::HashMap;
use std::time::{Duration, Instant};

use morf_scene::{Element, NodeHandle, Scene};

use crate::Handler;
use crate::events::KeyModifiers;

/// How long the first chords of a sequence wait for the next.
pub const SEQUENCE_TIMEOUT: Duration = Duration::from_millis(1500);

/// One chord: the modifiers held and the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub logo: bool,
    pub key: u32,
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
    pub fn pressed(keysym: u32, modifiers: KeyModifiers) -> Self {
        Self {
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            logo: modifiers.logo,
            key: fold(keysym),
        }
    }

    /// Whether the chord has no Ctrl, Alt or Super: a key that types.
    pub fn plain(self) -> bool {
        !self.ctrl && !self.alt && !self.logo
    }

    /// Parses `"ctrl+shift+k"`.
    pub fn parse(text: &str) -> Result<Self, String> {
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
pub fn parse_sequence(text: &str) -> Result<Vec<Chord>, String> {
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
pub struct NodeShortcuts {
    /// Whether they hold anywhere on the surface rather than only around
    /// focus.
    pub surface: bool,
    pub entries: Vec<(String, Vec<Chord>, Handler)>,
}

/// The first chords of a sequence, held on one surface.
#[derive(Default)]
pub struct Pending {
    pub root: Option<NodeHandle>,
    pub chords: Vec<Chord>,
    pub since: Option<Instant>,
}

/// Whether a node is a place keys type into.
pub fn types_keys(element: Option<Element>) -> bool {
    matches!(element, Some(Element::TextInput | Element::Terminal))
}

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
pub fn tap_of(keysym: u32) -> Option<u32> {
    Some(match keysym {
        0xffe9 | 0xffea | 0xfe03 => 0xffe9,
        0xffeb | 0xffec | 0xffe7 | 0xffe8 => 0xffeb,
        0xffe3 | 0xffe4 => 0xffe3,
        0xffe1 | 0xffe2 => 0xffe1,
        _ => return None,
    })
}

/// Whether a keysym is only a modifier going down.
pub fn modifier_only(keysym: u32) -> bool {
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

/// Whether a press of `chord` is for the shortcuts at all, with keys going
/// to a node that is `element`: a field keeps its editing chords and a
/// terminal its program's keys (only Super reaches past a terminal), and
/// plain keys are typing wherever keys type.
pub fn reaches_shortcuts(element: Option<Element>, chord: Chord) -> bool {
    let kept = (element == Some(Element::TextInput) && editing_chord(chord))
        || (element == Some(Element::Terminal) && !chord.logo);
    !(kept || (types_keys(element) && chord.plain() && !beyond_typing(chord.key)))
}

/// A modifier's tap as the chord a shortcut names it by (`"alt"`).
pub fn tap_chord(key: u32) -> Chord {
    Chord {
        ctrl: false,
        shift: false,
        alt: false,
        logo: false,
        key,
    }
}

impl Pending {
    /// The chords held for `root`, if they are still waiting.
    pub fn held_for(self, root: NodeHandle) -> Vec<Chord> {
        let fresh = self.root == Some(root)
            && self
                .since
                .is_some_and(|since| since.elapsed() < SEQUENCE_TIMEOUT);
        if fresh { self.chords } else { Vec::new() }
    }

    /// `sequence` held on `root`, waiting for its next chord.
    pub fn holding(root: NodeHandle, sequence: &[Chord]) -> Self {
        Self {
            root: Some(root),
            chords: sequence.to_vec(),
            since: Some(Instant::now()),
        }
    }
}

/// The shortcuts `sequence` runs on the surface whose tree is `root`, with
/// `target` the node keys go to there: first those on the target and its
/// ancestors, nearest first, then those of any node on the surface whose
/// table says `scope = "surface"`, in tree order. Also whether `sequence`
/// begins a longer one there.
pub fn matching(
    scene: &Scene,
    shortcuts: &HashMap<NodeHandle, NodeShortcuts>,
    root: NodeHandle,
    target: Option<NodeHandle>,
    sequence: &[Chord],
) -> (Vec<(NodeHandle, String, Handler)>, bool) {
    let mut owners = Vec::new();
    let mut current = target.or(Some(root));
    while let Some(node) = current {
        if shortcuts.contains_key(&node) && scene.can_hold_focus(node) {
            owners.push(node);
        }
        current = scene.parent(node).ok().flatten();
    }
    // In tree order, so the nearer to the top of the surface wins.
    let surface = scene.focus_nodes(root, |node| {
        shortcuts.get(&node).is_some_and(|s| s.surface) && !owners.contains(&node)
    });
    owners.extend(surface);
    let mut exact = Vec::new();
    let mut prefix = false;
    for node in owners {
        for (name, chords, handler) in &shortcuts[&node].entries {
            if chords.as_slice() == sequence {
                exact.push((node, name.clone(), handler.clone()));
            } else if chords.len() > sequence.len() && chords.starts_with(sequence) {
                prefix = true;
            }
        }
    }
    (exact, prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_parse_with_their_modifiers_and_fold_shifted_letters() {
        let chord = Chord::parse("ctrl+shift+k").unwrap();
        assert!(chord.ctrl && chord.shift && !chord.alt);
        let pressed = Chord::pressed(
            0x4b,
            KeyModifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(chord, pressed, "Shift+K arrives as `K`");
        assert_eq!(parse_sequence("ctrl+k ctrl+s").unwrap().len(), 2);
        assert_eq!(
            Chord::parse("alt").unwrap().key,
            0xffe9,
            "a modifier alone is its tap"
        );
        assert!(Chord::parse("hyper+x").is_err());
    }

    #[test]
    fn typing_keeps_its_keys_and_a_terminal_all_but_super() {
        let plain = Chord::parse("a").unwrap();
        let copy = Chord::parse("ctrl+c").unwrap();
        let launcher = Chord::parse("super+space").unwrap();
        assert!(reaches_shortcuts(None, plain));
        assert!(!reaches_shortcuts(Some(Element::TextInput), plain));
        assert!(!reaches_shortcuts(Some(Element::TextInput), copy));
        assert!(!reaches_shortcuts(Some(Element::Terminal), copy));
        assert!(reaches_shortcuts(Some(Element::Terminal), launcher));
        assert!(reaches_shortcuts(
            Some(Element::TextInput),
            Chord::parse("F5").unwrap()
        ));
    }
}
