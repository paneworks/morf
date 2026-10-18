//! Scene nodes from Lua: making them, writing and animating their
//! properties, the signals bindings follow, and the handle a node is.

use luna::{Callback, CallbackReturn, Context, Table, UserData, UserRef, Value as LuaValue};
use std::error::Error as StdError;
use std::fmt;
use std::rc::Rc;

use morf_scene::{Behavior, Element, NodeHandle, Value as SceneValue};

use crate::{api::ui::bindings::*, runtime::host_types::*, runtime::state::*, vm::loader::*};

mod metatable;
mod methods;

pub(crate) use metatable::{node_metatable, refuse_runtime_owned};
use methods::{terminal_method, text_input_method};

pub(crate) fn create_node(state: &Rc<StateCell<ReactiveState>>, element: Element) -> NodeHandle {
    let mut state = state.borrow_mut();
    state.revisions_mut().bump_scene();
    state.scene_mut().create(element)
}

/// A property of `node` changed: the scene moved on, and -- unless nothing
/// shows the node -- owes a paint.
fn bump_revision(state: &mut ReactiveState, node: NodeHandle, property: &str) {
    state.revisions_mut().bump_scene();
    if !state.scene().change_is_shown(node, property) {
        state.revisions_mut().bump_hidden();
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
        .tracking()
        .property_signal(&(node, property.to_owned(), target))
    else {
        return Ok(());
    };
    let value = IpcValue::Integer(state.revisions_mut().bump_property());
    if let Some(active) = state.tracking_mut().capture() {
        active.writes.push((signal, value.clone()));
        state.reactive_mut().mirror(signal, value);
        Ok(())
    } else {
        state.reactive_mut().write(signal, value)
    }
}

pub(crate) fn assign_scene_property(
    state: &mut ReactiveState,
    node: NodeHandle,
    property: &str,
    value: SceneValue,
) -> Result<(), String> {
    refuse_claimed(state, node, property)?;
    let named = glass_fields(state, node, property, &value);
    for field in &named {
        refuse_claimed(state, node, field)?;
    }
    write_scene_property(state, node, property, &named, |scene| {
        scene.assign(node, property, value)
    })
}

/// Aims a claimed property at its winning claim's target by `motion`: the
/// one write claims make, told to whatever reads the property.
pub(crate) fn retarget_scene_property(
    state: &mut ReactiveState,
    node: NodeHandle,
    property: &str,
    value: SceneValue,
    motion: Option<morf_scene::Motion>,
) -> Result<(), String> {
    write_scene_property(state, node, property, &[], |scene| {
        scene.retarget(node, property, value, motion)
    })
}

/// Refuses a write to a property `ui.claims` owns, the way a read-only
/// write is refused: its winning claim decides where it goes.
pub(crate) fn refuse_claimed(
    state: &ReactiveState,
    node: NodeHandle,
    property: &str,
) -> Result<(), String> {
    if state.animation().claims.is_claimed(node, property) {
        return Err(format!(
            "`{property}` is claimed: ui.claims owns it while its claims stand"
        ));
    }
    Ok(())
}

/// The flat properties a `glass` table written to `node` also sets.
fn glass_fields(
    state: &ReactiveState,
    node: NodeHandle,
    property: &str,
    value: &SceneValue,
) -> Vec<&'static str> {
    match value {
        SceneValue::Map(fields)
            if property == "glass" && state.scene().has_property(node, "glass") == Ok(true) =>
        {
            morf_scene::GLASS_KEYS
                .iter()
                .filter(|(key, _)| fields.contains_key(*key))
                .map(|(_, named)| *named)
                .collect()
        }
        _ => Vec::new(),
    }
}

/// A property's current value and target.
fn values(
    state: &ReactiveState,
    node: NodeHandle,
    property: &str,
) -> Result<(SceneValue, SceneValue), String> {
    let scene = state.scene();
    let current = scene.current(node, property).map_err(|e| e.to_string())?;
    let target = scene.target(node, property).map_err(|e| e.to_string())?;
    Ok((current.clone(), target.clone()))
}

fn write_scene_property(
    state: &mut ReactiveState,
    node: NodeHandle,
    property: &str,
    also: &[&'static str],
    write: impl FnOnce(&mut morf_scene::Scene) -> Result<(), morf_scene::SceneError>,
) -> Result<(), String> {
    let old = values(state, node, property)?;
    let also_old = also
        .iter()
        .map(|named| Ok((*named, values(state, node, named)?)))
        .collect::<Result<Vec<_>, String>>()?;
    write(state.scene_mut()).map_err(|error| error.to_string())?;
    // A path that draws a channel: watched for the channel's writes.
    if property == "series" {
        state.channels_mut().note_series(node);
    }
    // Text set in runs may hold links, which the layout places.
    if matches!(property, "spans" | "markup") {
        state.layout_hooks_mut().link_text(node);
    }
    announce_change(state, node, property, old)?;
    for (named, old) in also_old {
        announce_change(state, node, named, old)?;
    }
    Ok(())
}

/// Tells whatever follows `property` that it moved from `old`, its current
/// value and its target, if it did.
fn announce_change(
    state: &mut ReactiveState,
    node: NodeHandle,
    property: &str,
    (old_current, old_target): (SceneValue, SceneValue),
) -> Result<(), String> {
    let current_changed = state
        .scene()
        .current(node, property)
        .map_err(|error| error.to_string())?
        != &old_current;
    let target_changed = state
        .scene()
        .target(node, property)
        .map_err(|error| error.to_string())?
        != &old_target;
    if current_changed || target_changed {
        bump_revision(state, node, property);
        // A binding that reads this property is now stale. Inside a handler
        // the flush comes when the handler returns; outside one, at the next
        // signal write or clock tick, as it always did.
        state.tracking_mut().defer_flush();
    }
    if current_changed {
        bump_property_signal(state, node, property, false)?;
        // `focus = true` asks for focus; false gives it back. A text input's
        // own claims are settled by the text inputs, which focus follows.
        if property == "focus" && state.scene().element(node).ok() != Some(Element::TextInput) {
            let on = state.scene().bool_value(node, "focus").unwrap_or(false);
            crate::api::input::focus::request_by_property(state, node, on);
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
    refuse_claimed(state, node, property)?;
    let old_current = state
        .scene()
        .current(node, property)
        .map_err(|error| error.to_string())?
        .clone();
    let old_target = state
        .scene()
        .target(node, property)
        .map_err(|error| error.to_string())?
        .clone();
    state
        .scene_mut()
        .animate_from(node, property, from, to, behavior)
        .map_err(|error| error.to_string())?;
    if state
        .scene()
        .current(node, property)
        .map_err(|error| error.to_string())?
        != &old_current
    {
        bump_revision(state, node, property);
        bump_property_signal(state, node, property, false)?;
    }
    if state
        .scene()
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
    state: Rc<StateCell<ReactiveState>>,
    handle: NodeHandle,
) -> UserData<'gc> {
    let existing = state.borrow().metatables().node();
    let metatable = match existing {
        Some(stashed) => ctx.fetch(&stashed),
        None => {
            let built = node_metatable(ctx, Rc::clone(&state));
            state
                .borrow_mut()
                .metatables_mut()
                .set_node(ctx.stash(built));
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

impl From<String> for HostError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl From<morf_io::Error> for HostError {
    fn from(error: morf_io::Error) -> Self {
        Self(error.into_message())
    }
}
