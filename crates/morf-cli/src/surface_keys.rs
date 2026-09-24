//! Where a key goes: into the focused node of a surface's subtree.

use morf_lua::{KeyModifiers, Runtime};
use morf_scene::NodeHandle;
use morf_wayland::SurfaceRole;

use crate::surfaces::*;

/// What happened to a key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KeyAction {
    /// Pressed; `repeat` when it is the keyboard repeating a held key.
    Press { repeat: bool },
    /// Released.
    Release,
}

impl KeyAction {
    /// The action a compositor key event describes.
    pub(crate) fn of(pressed: bool, repeat: bool) -> Self {
        if pressed {
            Self::Press { repeat }
        } else {
            Self::Release
        }
    }
}

/// One key pressed or released on a surface: into its subtree, remembering
/// what has focus.
pub(crate) fn surface_key(
    runtime: &mut Runtime,
    state: &mut SurfaceEventState,
    surface: SurfaceRole,
    action: KeyAction,
    keysym: u32,
    text: Option<&str>,
    modifiers: morf_wayland::KeyModifiers,
) -> bool {
    let Some(root) = surface_root(
        surface,
        state.primary_root,
        &state.popup_surfaces,
        &state.floating_surfaces,
        &state.layer_surfaces,
    ) else {
        return false;
    };
    let mut focused = state.input.focused.get(&surface).copied();
    let repaint = dispatch_key_in_subtree(
        runtime,
        root,
        &mut focused,
        action,
        keysym,
        text,
        key_modifiers(modifiers),
    );
    match focused {
        Some(node) => state.input.focused.insert(surface, node),
        None => state.input.focused.remove(&surface),
    };
    repaint
}

/// Routes one key into a surface subtree, keeping its focus.
///
/// Tab moves to the next focusable node and off the end again; anything else
/// goes to whatever holds focus, or to the first thing that can take it. A
/// release goes where a press would go now and never moves focus — so Tab's
/// own release lands on the node Tab moved to.
///
/// This exists as a function because the lock screen had its own copy that did
/// neither — no traversal and no persistence, so every key went to the first
/// focusable node in the tree. On the one surface whose entire purpose is to
/// accept a password, a second field could not be reached at all.
///
/// Returns whether anything changed enough to want a repaint.
pub(crate) fn dispatch_key_in_subtree(
    runtime: &mut Runtime,
    root: NodeHandle,
    focused: &mut Option<NodeHandle>,
    action: KeyAction,
    keysym: u32,
    text: Option<&str>,
    modifiers: KeyModifiers,
) -> bool {
    const TAB: u32 = 0xff09;
    // A text input holding the keyboard keeps it until something takes it:
    // a click elsewhere, a Tab, or the configuration writing `focus`.
    let current = runtime
        .focused_text_input_in(root)
        .or(focused.filter(|node| runtime.node_in_subtree(root, *node)));
    let repeat = match action {
        KeyAction::Release => {
            let Some(node) = current.or_else(|| runtime.first_key_target_in(root)) else {
                return false;
            };
            return runtime.dispatch_key_release(node, keysym, text, modifiers);
        }
        KeyAction::Press { repeat } => repeat,
    };
    if keysym == TAB {
        *focused = runtime.next_key_target_in(root, current);
        runtime.set_key_focus(*focused);
        return true;
    }
    let Some(node) = current.or_else(|| runtime.first_key_target_in(root)) else {
        return false;
    };
    *focused = Some(node);
    if runtime.is_text_input(node) {
        runtime.set_key_focus(Some(node));
    }
    runtime.dispatch_key_press(node, keysym, text, modifiers, repeat)
}

/// The compositor's modifier state, as the runtime reads it.
pub(crate) fn key_modifiers(modifiers: morf_wayland::KeyModifiers) -> KeyModifiers {
    KeyModifiers {
        ctrl: modifiers.ctrl,
        shift: modifiers.shift,
        alt: modifiers.alt,
        logo: modifiers.logo,
    }
}
