//! Where a key goes: into the focused node of a surface's subtree.

use morf_lua::{FocusReason, KeyModifiers, Runtime};
use morf_scene::NodeHandle;
use morf_app::WindowId;

use crate::surfaces::*;

/// What happened to a key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyAction {
    /// Pressed; `repeat` when it is the keyboard repeating a held key.
    Press { repeat: bool },
    /// Released.
    Release,
}

impl KeyAction {
    /// The action a compositor key event describes.
    pub fn of(pressed: bool, repeat: bool) -> Self {
        if pressed {
            Self::Press { repeat }
        } else {
            Self::Release
        }
    }
}

/// One key pressed or released on a surface: into its subtree, remembering
/// what has focus.
pub fn surface_key(
    runtime: &mut Runtime,
    state: &mut SurfaceEventState,
    surface: WindowId,
    action: KeyAction,
    keysym: u32,
    text: Option<&str>,
    modifiers: morf_app::KeyModifiers,
) -> bool {
    let Some(root) = surface_root(
        surface,
        state.primary_root,
        &state.windows,
    ) else {
        return false;
    };
    runtime.set_held_modifiers(key_modifiers(modifiers));
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
pub fn dispatch_key_in_subtree(
    runtime: &mut Runtime,
    root: NodeHandle,
    focused: &mut Option<NodeHandle>,
    action: KeyAction,
    keysym: u32,
    text: Option<&str>,
    modifiers: KeyModifiers,
) -> bool {
    const TAB: u32 = 0xff09;
    const ISO_LEFT_TAB: u32 = 0xfe20;
    // A text input holding the keyboard keeps it until something takes it:
    // a click elsewhere, a Tab, or the configuration writing `focus`. Then
    // the node with focus; then whatever this surface last sent keys to.
    let current = runtime
        .focused_text_input_in(root)
        .or_else(|| runtime.focus_owner(root))
        .or(focused.filter(|node| runtime.node_in_subtree(root, *node)));
    // A modifier tapped alone (Alt, to reach a menu bar).
    let tap_target = current.or_else(|| runtime.first_key_target_in(root));
    let pressed = matches!(action, KeyAction::Press { repeat: false });
    if !matches!(action, KeyAction::Press { repeat: true })
        && runtime.note_key_for_tap(root, tap_target, keysym, pressed)
    {
        return true;
    }
    let repeat = match action {
        KeyAction::Release => {
            let Some(node) = current
                .or_else(|| runtime.first_key_target_in(root))
                .and_then(|node| runtime.key_route(node))
            else {
                return false;
            };
            return runtime.dispatch_key_release(node, keysym, text, modifiers);
        }
        KeyAction::Press { repeat } => repeat,
    };
    // Escape closes the surface's top overlay before anything else hears it.
    const ESCAPE: u32 = 0xff1b;
    if keysym == ESCAPE && runtime.overlay_escape(root) {
        return true;
    }
    // Shortcuts before the node: those around focus, then the surface's.
    let target = current.or_else(|| runtime.first_key_target_in(root));
    if runtime.dispatch_shortcut(root, target, keysym, modifiers) {
        return true;
    }
    // A node that set `tab_navigation = false` keeps Tab (and Shift+Tab,
    // which arrives as ISO_Left_Tab) as a key of its own.
    let keeps_tab = current.is_some_and(|node| !runtime.tab_navigates(node));
    let plain = !modifiers.ctrl && !modifiers.alt && !modifiers.logo;
    if (keysym == TAB || keysym == ISO_LEFT_TAB) && plain && !keeps_tab {
        let backwards = keysym == ISO_LEFT_TAB || modifiers.shift;
        // Inside a modal overlay, Tab stays inside it.
        let next = runtime.next_focus_in(runtime.focus_root(root), current, backwards);
        *focused = next;
        runtime.set_focus(root, next, FocusReason::Keyboard);
        return true;
    }
    // Nothing has focus: each node that takes keys is offered it in turn,
    // so a group that only wants keys from inside it (a toolbar) does not
    // swallow one a list after it would take.
    if current.is_none() {
        for node in runtime.key_targets_in_root(root) {
            let Some(route) = runtime.key_route(node) else { continue };
            if runtime.dispatch_key_press_bubbling(route, keysym, text, modifiers, repeat) {
                *focused = Some(node);
                return true;
            }
        }
        return false;
    }
    let Some(node) = current else {
        return false;
    };
    *focused = Some(node);
    // A focused button has no keys of its own: Enter clicks it, and the rest
    // go up to whatever around it takes keys.
    if runtime.activate_by_key(node, keysym, modifiers) {
        return true;
    }
    let Some(node) = runtime.key_route(node) else {
        return false;
    };
    if runtime.is_text_input(node) {
        runtime.set_key_focus(Some(node));
    }
    runtime.dispatch_key_press_bubbling(node, keysym, text, modifiers, repeat)
}

/// The compositor's modifier state, as the runtime reads it.
pub fn key_modifiers(modifiers: morf_app::KeyModifiers) -> KeyModifiers {
    KeyModifiers {
        ctrl: modifiers.ctrl,
        shift: modifiers.shift,
        alt: modifiers.alt,
        logo: modifiers.logo,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use morf_value::IpcValue;

    fn tab_setup(first: &str) -> (Runtime, NodeHandle, NodeHandle) {
        let mut runtime = Runtime::default();
        let source = format!(
            r#"
                local ui = require("morf.ui")
                local keys = {{}}
                ui.Item {{
                    ui.TextInput {{
                        width = 100, height = 20, focus = true, {first}
                        on_key_pressed = function(keysym) keys[#keys + 1] = keysym end,
                    }},
                    ui.TextInput {{ width = 100, height = 20 }},
                }}
                morf.ipc.keys = function() return #keys end
            "#
        );
        runtime.execute("tab.lua", source.as_bytes()).unwrap();
        let root = runtime.scene().roots()[0];
        let children = runtime.scene().children(root).unwrap().to_vec();
        (runtime, root, children[0])
    }

    #[test]
    fn tab_moves_focus_unless_the_field_keeps_it() {
        const TAB: u32 = 0xff09;
        let (mut runtime, root, first) = tab_setup("");
        let mut focused = Some(first);
        dispatch_key_in_subtree(
            &mut runtime,
            root,
            &mut focused,
            KeyAction::Press { repeat: false },
            TAB,
            Some("\t"),
            Default::default(),
        );
        assert_ne!(focused, Some(first));
        assert_eq!(
            runtime.call_ipc("keys", &[]).unwrap(),
            [IpcValue::Integer(0)]
        );

        let (mut runtime, root, first) = tab_setup("tab_navigation = false,");
        let mut focused = Some(first);
        dispatch_key_in_subtree(
            &mut runtime,
            root,
            &mut focused,
            KeyAction::Press { repeat: false },
            TAB,
            Some("\t"),
            Default::default(),
        );
        assert_eq!(focused, Some(first));
        assert_eq!(
            runtime.call_ipc("keys", &[]).unwrap(),
            [IpcValue::Integer(1)]
        );
        assert_eq!(runtime.scene().string_value(first, "text").unwrap(), "");
    }

    #[test]
    fn shift_tab_walks_back_and_tab_draws_the_ring() {
        const ISO_LEFT_TAB: u32 = 0xfe20;
        let (mut runtime, root, first) = tab_setup("");
        let second = runtime.scene().children(root).unwrap()[1];
        let mut focused = Some(first);
        dispatch_key_in_subtree(
            &mut runtime,
            root,
            &mut focused,
            KeyAction::Press { repeat: false },
            ISO_LEFT_TAB,
            None,
            KeyModifiers {
                shift: true,
                ..Default::default()
            },
        );
        // From the first, back is around to the last.
        assert_eq!(focused, Some(second));
        assert!(runtime.scene().bool_value(second, "visual_focus").unwrap());
        assert!(!runtime.scene().bool_value(first, "focused").unwrap());
    }
}
