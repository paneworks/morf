//! `ui.Terminal`: a program on a pseudo-terminal, as a node.
//!
//! The terminals themselves -- emulators, ptys, feeding, fitting, keys and
//! the pointer -- are `morf_terminal::hub`'s. This is the runtime's side:
//! the hub lives in the reactive state, and what it leaves behind (node
//! properties to write, warnings, screens hung) is applied here, so the
//! scene's bindings see every write.

use morf_layout::Layout;
use morf_scene::{NodeHandle, Scene};
use morf_terminal::hub::{Call, Callbacks, Hub, Spec};
use morf_terminal::{Modifiers, MouseAction};
use morf_text::TextSystem;

use crate::scene_bindings::assign_scene_property;
use crate::state::ReactiveState;
use morf_runtime::Handler;

pub(crate) use morf_terminal::hub::DEFAULT_SCROLLBACK;

/// How to start a terminal's program.
pub(crate) type TerminalSpec = Spec;
/// A terminal's callbacks.
pub(crate) type TerminalCallbacks = Callbacks<Handler>;
/// A callback owed.
pub(crate) type TerminalCall = Call<Handler>;
/// The runtime's terminals.
pub(crate) type TerminalHub = Hub<Handler>;

/// Runs `work` on the hub and the scene, then applies what it left.
fn with_hub<R>(
    state: &mut ReactiveState,
    work: impl FnOnce(&mut TerminalHub, &mut Scene) -> R,
) -> R {
    let result = work(&mut state.terminals, &mut state.engine.scene);
    let effects = state.terminals.take_effects();
    state.engine.revisions.scene_revision = state
        .engine
        .revisions
        .scene_revision
        .wrapping_add(effects.screens);
    for (node, property, value) in effects.properties {
        let _ = assign_scene_property(state, node, property, value);
    }
    for warning in effects.warnings {
        state.log(crate::LogLevel::Warn, warning);
    }
    result
}

/// Feeds each terminal what its program wrote. Returns the callbacks owed,
/// whether any screen changed, and whether more is waiting.
pub(crate) fn pump(state: &mut ReactiveState) -> (Vec<TerminalCall>, bool, bool) {
    with_hub(state, |hub, scene| hub.pump(scene))
}

/// Fits each laid-out terminal's grid to its box, with the cell the font
/// makes, starting its program the first time. Returns whether anything
/// changed.
pub(crate) fn sync(state: &mut ReactiveState, layout: &Layout, text: &mut TextSystem) -> bool {
    let mut changed = false;
    for node in state.terminals.nodes() {
        let Some(geometry) = layout.geometry(node) else {
            continue;
        };
        let (Ok(family), Ok(size), Ok(_)) = (
            state
                .scene
                .string_value(node, "font_family")
                .map(str::to_owned),
            state.scene.number(node, "font_size"),
            state.scene.number(node, "padding"),
        ) else {
            continue;
        };
        let metrics = text.terminal_metrics(&family, size);
        let size = (geometry.width, geometry.height);
        changed |= with_hub(state, |hub, scene| hub.fit(scene, node, size, metrics));
    }
    changed
}

pub(crate) fn key(
    state: &mut ReactiveState,
    node: NodeHandle,
    keysym: u32,
    text: Option<&str>,
    modifiers: Modifiers,
) -> bool {
    with_hub(state, |hub, scene| {
        hub.key(scene, node, keysym, text, modifiers)
    })
}

pub(crate) fn pointer(
    state: &mut ReactiveState,
    node: NodeHandle,
    action: MouseAction,
    button: Option<u32>,
    local: (f64, f64),
) -> bool {
    with_hub(state, |hub, scene| {
        hub.pointer(scene, node, action, button, local)
    })
}

pub(crate) fn wheel(
    state: &mut ReactiveState,
    node: NodeHandle,
    local: (f64, f64),
    pixels: f64,
    steps: i32,
) -> bool {
    with_hub(state, |hub, scene| {
        hub.wheel(scene, node, local, pixels, steps)
    })
}

pub(crate) fn set_focus(state: &mut ReactiveState, node: Option<NodeHandle>) -> bool {
    with_hub(state, |hub, scene| hub.set_focus(scene, node))
}

pub(crate) fn write(
    state: &mut ReactiveState,
    node: NodeHandle,
    bytes: Vec<u8>,
) -> Result<(), String> {
    state.terminals.write(node, bytes)
}

pub(crate) fn paste(state: &mut ReactiveState, node: NodeHandle, text: &str) -> Result<(), String> {
    state.terminals.paste(node, text)
}

pub(crate) fn kill(state: &mut ReactiveState, node: NodeHandle, signal: i32) -> bool {
    state.terminals.kill(node, signal)
}

pub(crate) fn scroll(state: &mut ReactiveState, node: NodeHandle, lines: i32) -> bool {
    with_hub(state, |hub, scene| hub.scroll(scene, node, lines))
}

pub(crate) fn text(state: &ReactiveState, node: NodeHandle) -> Option<String> {
    state.terminals.text(node)
}

pub(crate) fn selection(state: &ReactiveState, node: NodeHandle) -> Option<String> {
    state.terminals.selection(node)
}

pub(crate) fn clear_selection(state: &mut ReactiveState, node: NodeHandle) -> bool {
    with_hub(state, |hub, scene| hub.clear_selection(scene, node))
}

pub(crate) fn pid(state: &ReactiveState, node: NodeHandle) -> Option<u32> {
    state.terminals.pid(node)
}
