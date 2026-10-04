//! Scene nodes from Lua: making them, writing and animating their
//! properties, the signals bindings follow, and the handle a node is.

use luna::{Callback, CallbackReturn, Context, Table, UserData, UserRef, Value as LuaValue};
use std::cell::RefCell;
use std::error::Error as StdError;
use std::fmt;
use std::rc::Rc;

use morf_scene::{Behavior, Element, NodeHandle, Value as SceneValue};

use crate::{reactive_bindings::*, serialization::*, state::*, surface_types::*};

mod metatable;
mod methods;

pub(crate) use metatable::{node_metatable, refuse_runtime_owned};
use methods::{terminal_method, text_input_method};

pub(crate) fn create_node(state: &Rc<RefCell<ReactiveState>>, element: Element) -> NodeHandle {
    let mut state = state.borrow_mut();
    state.revisions.scene_revision = state.revisions.scene_revision.wrapping_add(1);
    state.scene.create(element)
}

/// A property of `node` changed: the scene moved on, and -- unless nothing
/// shows the node -- owes a paint.
fn bump_revision(state: &mut ReactiveState, node: NodeHandle, property: &str) {
    state.revisions.scene_revision = state.revisions.scene_revision.wrapping_add(1);
    if !state.scene.change_is_shown(node, property) {
        state.revisions.hidden_revisions = state.revisions.hidden_revisions.wrapping_add(1);
    }
}

pub(crate) use morf_runtime::layout::{LAYOUT_POSITION, LAYOUT_SIZE};

/// A node's `contains_pointer`: kept by the runtime for the nodes something
/// read it of, and the pseudo-property a binding reading it depends on.
pub(crate) const CONTAINS_POINTER: &str = "contains_pointer";

pub(crate) fn bump_property_signal(
    state: &mut ReactiveState,
    node: NodeHandle,
    property: &str,
    target: bool,
) -> Result<(), String> {
    let Some(signal) = state
        .property_signals
        .get(&(node, property.to_owned(), target))
        .copied()
    else {
        return Ok(());
    };
    state.revisions.property_revision = state.revisions.property_revision.wrapping_add(1);
    let value = IpcValue::Integer(state.revisions.property_revision);
    if let Some(active) = &mut state.active {
        active.writes.push((signal, value.clone()));
    } else {
        state
            .reactive
            .graph
            .as_mut()
            .ok_or_else(|| "reactive graph is already running".to_owned())?
            .write(signal, value.clone())
            .map_err(|error| error.to_string())?;
    }
    state.reactive.values.insert(signal, value);
    Ok(())
}

pub(crate) fn assign_scene_property(
    state: &mut ReactiveState,
    node: NodeHandle,
    property: &str,
    value: SceneValue,
) -> Result<(), String> {
    let old_current = state
        .scene
        .current(node, property)
        .map_err(|error| error.to_string())?
        .clone();
    let old_target = state
        .scene
        .target(node, property)
        .map_err(|error| error.to_string())?
        .clone();
    state
        .scene
        .assign(node, property, value)
        .map_err(|error| error.to_string())?;
    // A path that draws a channel: watched for the channel's writes.
    if property == "series" {
        state.channels.note_series(node);
    }
    // Text set in runs may hold links, which the layout places.
    if matches!(property, "spans" | "markup") {
        state.linked_texts.insert(node);
    }
    let current_changed = state
        .scene
        .current(node, property)
        .map_err(|error| error.to_string())?
        != &old_current;
    let target_changed = state
        .scene
        .target(node, property)
        .map_err(|error| error.to_string())?
        != &old_target;
    if current_changed || target_changed {
        bump_revision(state, node, property);
        // A binding that reads this property is now stale. Inside a handler
        // the flush comes when the handler returns; outside one, at the next
        // signal write or clock tick, as it always did.
        state.flush_pending = true;
    }
    if current_changed {
        bump_property_signal(state, node, property, false)?;
        // `focus = true` asks for focus; false gives it back. A text input's
        // own claims are settled by the text inputs, which focus follows.
        if property == "focus" && state.scene.element(node).ok() != Some(Element::TextInput) {
            let on = state.scene.bool_value(node, "focus").unwrap_or(false);
            crate::api_focus::request_by_property(state, node, on);
        }
    }
    if target_changed {
        bump_property_signal(state, node, property, true)?;
    }
    Ok(())
}

pub(crate) fn animate_scene_property(
    state: &mut ReactiveState,
    node: NodeHandle,
    property: &str,
    from: SceneValue,
    to: SceneValue,
    behavior: Behavior,
) -> Result<(), String> {
    let old_current = state
        .scene
        .current(node, property)
        .map_err(|error| error.to_string())?
        .clone();
    let old_target = state
        .scene
        .target(node, property)
        .map_err(|error| error.to_string())?
        .clone();
    state
        .scene
        .animate_from(node, property, from, to, behavior)
        .map_err(|error| error.to_string())?;
    if state
        .scene
        .current(node, property)
        .map_err(|error| error.to_string())?
        != &old_current
    {
        bump_revision(state, node, property);
        bump_property_signal(state, node, property, false)?;
    }
    if state
        .scene
        .target(node, property)
        .map_err(|error| error.to_string())?
        != &old_target
    {
        bump_property_signal(state, node, property, true)?;
    }
    Ok(())
}

/// Wraps one scene node as a Lua handle.
///
/// The metatable is built once and shared by every node. Neither of its two
/// callbacks captures anything about a particular node — both take the node off
/// the stack as a `UserRef<NodeToken>` — so building a table and two closures
/// per node allocated three objects per node that were all identical.
pub(crate) fn node_userdata<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    handle: NodeHandle,
) -> UserData<'gc> {
    let existing = state.borrow().node_metatable.clone();
    let metatable = match existing {
        Some(stashed) => ctx.fetch(&stashed),
        None => {
            let built = node_metatable(ctx, Rc::clone(&state));
            state.borrow_mut().node_metatable = Some(ctx.stash(built));
            built
        }
    };
    let userdata = UserData::new_static(&ctx, NodeToken { handle });
    userdata.set_metatable(ctx, Some(metatable));
    userdata
}

fn busy() -> HostError {
    HostError("text inputs cannot be edited from inside a layout function".to_owned())
}

#[derive(Debug)]
pub(crate) struct HostError(pub(crate) String);

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl StdError for HostError {}
