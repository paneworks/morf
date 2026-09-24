//! Where a key goes: into the focused node of a surface's subtree.

use morf_lua::{KeyModifiers, Runtime};
use morf_scene::NodeHandle;
use morf_wayland::SurfaceRole;

use crate::surfaces::*;

/// One key pressed on a surface: into its subtree, remembering what has focus.
pub(crate) fn surface_key(
    runtime: &mut Runtime,
    state: &mut SurfaceEventState,
    surface: SurfaceRole,
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

/// Routes one key press into a surface subtree, keeping its focus.
///
/// Tab moves to the next focusable node and off the end again; anything else
/// goes to whatever holds focus, or to the first thing that can take it.
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
    // A node that set `tab_navigation = false` keeps Tab (and Shift+Tab,
    // which arrives as ISO_Left_Tab) as a key of its own.
    let keeps_tab = current.is_some_and(|node| !runtime.tab_navigates(node));
    if keysym == TAB && !keeps_tab {
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
    runtime.dispatch_key(node, keysym, text, modifiers)
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

#[cfg(test)]
mod tests {
    use super::*;
    use morf_lua::IpcValue;

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
}
