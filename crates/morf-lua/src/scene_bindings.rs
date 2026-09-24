use luna::{Callback, CallbackReturn, Context, Table, UserData, UserRef, Value as LuaValue};
use std::cell::RefCell;
use std::error::Error as StdError;
use std::fmt;
use std::rc::Rc;

use morf_scene::{Behavior, Element, NodeHandle, Value as SceneValue};

use crate::{reactive_bindings::*, serialization::*, state::*, surface_types::*};

pub(crate) fn create_node(state: &Rc<RefCell<ReactiveState>>, element: Element) -> NodeHandle {
    let mut state = state.borrow_mut();
    state.scene_revision = state.scene_revision.wrapping_add(1);
    state.scene.create(element)
}

/// The pseudo-property a binding depends on when it reads `layout_*`.
pub(crate) const LAYOUT_GEOMETRY: &str = "layout_geometry";

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
    state.property_revision = state.property_revision.wrapping_add(1);
    let value = IpcValue::Integer(state.property_revision);
    if let Some(active) = &mut state.active {
        active.writes.push((signal, value.clone()));
    } else {
        state
            .graph
            .as_mut()
            .ok_or_else(|| "reactive graph is already running".to_owned())?
            .write(signal, value.clone())
            .map_err(|error| error.to_string())?;
    }
    state.values.insert(signal, value);
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
        state.scene_revision = state.scene_revision.wrapping_add(1);
        // A binding that reads this property is now stale. Inside a handler
        // the flush comes when the handler returns; outside one, at the next
        // signal write or clock tick, as it always did.
        state.flush_pending = true;
    }
    if current_changed {
        bump_property_signal(state, node, property, false)?;
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
        state.scene_revision = state.scene_revision.wrapping_add(1);
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

pub(crate) fn node_metatable<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
) -> Table<'gc> {
    let read_state = Rc::clone(&state);
    let index = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (node, key): (UserRef<NodeToken>, String) = stack.consume(ctx)?;
        let element = read_state
            .borrow()
            .scene
            .element(node.handle)
            .map_err(|error| HostError(error.to_string()))?;
        if key == "item" && element == Element::Loader {
            let child = read_state
                .borrow()
                .scene
                .children(node.handle)
                .map_err(|error| HostError(error.to_string()))?
                .first()
                .copied();
            match child {
                Some(child) => {
                    stack.replace(ctx, node_userdata(ctx, Rc::clone(&read_state), child))
                }
                None => stack.replace(ctx, LuaValue::Nil),
            }
            return Ok(CallbackReturn::Return);
        }
        if element == Element::TextInput
            && let Some(method) = text_input_method(ctx, &read_state, &key)
        {
            stack.replace(ctx, method);
            return Ok(CallbackReturn::Return);
        }
        if element == Element::Terminal
            && let Some(method) = terminal_method(ctx, &read_state, &key)
        {
            stack.replace(ctx, method);
            return Ok(CallbackReturn::Return);
        }
        let key = if key == "active_async" && element == Element::Loader {
            "active".to_owned()
        } else {
            key
        };
        // The laid-out rectangle, as the last frame resolved it. Distinct
        // from `width`, which is what the node asked for and is zero for a
        // node sized by its parent or its children. A binding that reads
        // one of these re-runs when the frame moves it.
        if let Some(axis) = key.strip_prefix("layout_")
            && matches!(axis, "x" | "y" | "width" | "height")
        {
            let mut state = read_state.borrow_mut();
            if let Some(active) = &mut state.active {
                active
                    .property_reads
                    .insert((node.handle, LAYOUT_GEOMETRY.to_owned(), false));
            }
            let value =
                state
                    .transform_tracker
                    .geometry(node.handle)
                    .map_or(LuaValue::Nil, |geometry| {
                        LuaValue::Number(match axis {
                            "x" => geometry.x,
                            "y" => geometry.y,
                            "width" => geometry.width,
                            _ => geometry.height,
                        })
                    });
            stack.replace(ctx, value);
            return Ok(CallbackReturn::Return);
        }
        let (property, target) = key
            .strip_suffix("_target")
            .map_or((key.as_str(), false), |property| (property, true));
        let value = {
            let mut state = read_state.borrow_mut();
            if !state
                .scene
                .has_property(node.handle, property)
                .map_err(|error| HostError(error.to_string()))?
            {
                return Err(HostError(format!("unknown node property `{key}`")).into());
            }
            let property_key = (node.handle, property.to_owned(), target);
            let signal = state.property_signals.get(&property_key).copied();
            if let Some(active) = &mut state.active {
                if let Some(signal) = signal {
                    active.reads.insert(signal);
                } else {
                    active.property_reads.insert(property_key);
                }
            }
            if target {
                state.scene.target(node.handle, property)
            } else {
                state.scene.current(node.handle, property)
            }
            .map_err(|error| HostError(error.to_string()))?
            .clone()
        };
        stack.replace(ctx, scene_to_lua(ctx, &value).map_err(HostError)?);
        Ok(CallbackReturn::Return)
    });
    let new_index = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (node, property, value): (UserRef<NodeToken>, String, LuaValue) = stack.consume(ctx)?;
        let value = lua_to_scene(ctx, value, 0).map_err(HostError)?;
        // A write while the scene is borrowed by a layout pass -- from
        // inside a `ui.Layout` function -- is refused rather than allowed to
        // change the tree the pass is walking.
        let mut state = state.try_borrow_mut().map_err(|_| {
            HostError("nodes cannot be written to from inside a layout function".to_owned())
        })?;
        refuse_runtime_owned(&state, node.handle, &property).map_err(HostError)?;
        assign_scene_property(&mut state, node.handle, &property, value).map_err(HostError)?;
        // A text input takes a write in at once, so the caret a handler
        // reads back after setting `text` is already the one that text has.
        if state.scene.element(node.handle).ok() == Some(Element::TextInput) {
            crate::text_inputs::pull(&mut state, node.handle);
            if property == "focus" {
                crate::text_inputs::reconcile_focus(&mut state);
            }
        }
        Ok(CallbackReturn::Return)
    });
    let metatable = Table::new(&ctx);
    metatable.set_field(ctx, "__index", index);
    metatable.set_field(ctx, "__newindex", new_index);
    metatable
}

/// Refuses a configuration's write to a property the runtime keeps: a
/// `MouseArea`'s `hovered` and `pressed` say what the pointer is doing, and
/// a write would only be undone by the next pointer event.
pub(crate) fn refuse_runtime_owned(
    state: &ReactiveState,
    node: NodeHandle,
    property: &str,
) -> Result<(), String> {
    if matches!(property, "hovered" | "pressed")
        && state.scene.element(node).ok() == Some(Element::MouseArea)
    {
        return Err(format!(
            "MouseArea `{property}` is read-only: the pointer sets it"
        ));
    }
    if property == "links" && state.scene.element(node).ok() == Some(Element::Text) {
        return Err("Text `links` is read-only: the layout says where they are".to_owned());
    }
    if matches!(property, "status" | "error" | "frame_count")
        && state.scene.element(node).ok() == Some(Element::Image)
    {
        return Err(format!(
            "Image `{property}` is read-only: the runtime says what became of the source"
        ));
    }
    if matches!(
        property,
        "columns" | "rows" | "title" | "running" | "exit_code"
    ) && state.scene.element(node).ok() == Some(Element::Terminal)
    {
        return Err(format!(
            "Terminal `{property}` is read-only: the terminal and its program set it"
        ));
    }
    Ok(())
}

/// A terminal's methods, called as `term:write("ls\r")`.
fn terminal_method<'gc>(
    ctx: Context<'gc>,
    state: &Rc<RefCell<ReactiveState>>,
    name: &str,
) -> Option<Callback<'gc>> {
    use crate::terminals;
    let state = Rc::clone(state);
    let busy = || HostError("terminals cannot be used from inside a layout function".to_owned());
    Some(match name {
        // Bytes to the program, as if typed: `true`, or `false, why`.
        "write" | "paste" => {
            let paste = name == "paste";
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let (node, data): (UserRef<NodeToken>, luna::String) = stack.consume(ctx)?;
                if data.as_bytes().len() > morf_io::MAX_OUTGOING {
                    return Err(HostError("a terminal write is at most 4 MiB".into()).into());
                }
                let mut state = state.try_borrow_mut().map_err(|_| busy())?;
                let result = if paste {
                    terminals::paste(&mut state, node.handle, &data.display_lossy().to_string())
                } else {
                    terminals::write(&mut state, node.handle, data.as_bytes().to_vec())
                };
                match result {
                    Ok(()) => stack.replace(ctx, true),
                    Err(why) => stack.replace(ctx, (false, why.as_str())),
                }
                Ok(CallbackReturn::Return)
            })
        }
        // A signal to the program: `"TERM"` unless named; whether it ran.
        "kill" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (node, signal): (UserRef<NodeToken>, LuaValue) = stack.consume(ctx)?;
            let signal = match signal {
                LuaValue::Nil => morf_io::signal_number("TERM"),
                LuaValue::Integer(number) => morf_io::signal_number(&number.to_string()),
                LuaValue::String(name) => morf_io::signal_number(&name.display_lossy().to_string()),
                _ => None,
            }
            .ok_or_else(|| HostError("kill takes a signal name or number".into()))?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            stack.replace(ctx, terminals::kill(&mut state, node.handle, signal));
            Ok(CallbackReturn::Return)
        }),
        // Through the history: up by `lines`, down by a negative number,
        // back to the bottom with none. Whether the view moved.
        "scroll" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (node, lines): (UserRef<NodeToken>, Option<i64>) = stack.consume(ctx)?;
            let lines = lines.unwrap_or(0).clamp(-100_000, 100_000) as i32;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            stack.replace(ctx, terminals::scroll(&mut state, node.handle, lines));
            Ok(CallbackReturn::Return)
        }),
        // What the screen shows, as text: one line per row.
        "text" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let state = state.try_borrow().map_err(|_| busy())?;
            let text = terminals::text(&state, node.handle).unwrap_or_default();
            stack.replace(ctx, luna::String::from_slice(&ctx, text.as_bytes()));
            Ok(CallbackReturn::Return)
        }),
        // The text selected with the pointer, or nil.
        "selection" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let state = state.try_borrow().map_err(|_| busy())?;
            match terminals::selection(&state, node.handle) {
                Some(text) => stack.replace(ctx, luna::String::from_slice(&ctx, text.as_bytes())),
                None => stack.replace(ctx, LuaValue::Nil),
            }
            Ok(CallbackReturn::Return)
        }),
        "clear_selection" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            stack.replace(ctx, terminals::clear_selection(&mut state, node.handle));
            Ok(CallbackReturn::Return)
        }),
        "pid" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let state = state.try_borrow().map_err(|_| busy())?;
            match terminals::pid(&state, node.handle) {
                Some(pid) => stack.replace(ctx, i64::from(pid)),
                None => stack.replace(ctx, LuaValue::Nil),
            }
            Ok(CallbackReturn::Return)
        }),
        _ => return None,
    })
}

/// A text input's methods, called as `input:select(0, 4)`.
///
/// Offsets are bytes, as `cursor_position` is: the number of bytes before the
/// place meant, so `text:sub(1, n)` is what lies before offset `n`.
fn text_input_method<'gc>(
    ctx: Context<'gc>,
    state: &Rc<RefCell<ReactiveState>>,
    name: &str,
) -> Option<Callback<'gc>> {
    use crate::text_inputs;
    let state = Rc::clone(state);
    let offset = |value: i64| value.max(0) as usize;
    Some(match name {
        "select" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (node, start, end): (UserRef<NodeToken>, i64, i64) = stack.consume(ctx)?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            text_inputs::select(&mut state, node.handle, offset(start), offset(end));
            Ok(CallbackReturn::Return)
        }),
        "select_all" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            text_inputs::select_all(&mut state, node.handle);
            Ok(CallbackReturn::Return)
        }),
        "deselect" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            let cursor = state
                .scene
                .number(node.handle, "cursor_position")
                .unwrap_or(0.0);
            text_inputs::select(&mut state, node.handle, cursor as usize, cursor as usize);
            Ok(CallbackReturn::Return)
        }),
        "insert" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (node, text): (UserRef<NodeToken>, String) = stack.consume(ctx)?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            let edited = text_inputs::insert(&mut state, node.handle, &text);
            stack.replace(ctx, edited);
            Ok(CallbackReturn::Return)
        }),
        "selected_text" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            let text = text_inputs::selected_text(&mut state, node.handle);
            stack.replace(ctx, luna::String::from_slice(&ctx, text.as_bytes()));
            Ok(CallbackReturn::Return)
        }),
        "undo" | "redo" => {
            let redo = name == "redo";
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let node: UserRef<NodeToken> = stack.consume(ctx)?;
                let mut state = state.try_borrow_mut().map_err(|_| busy())?;
                let changed = text_inputs::history(&mut state, node.handle, redo);
                stack.replace(ctx, changed);
                Ok(CallbackReturn::Return)
            })
        }
        _ => return None,
    })
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
