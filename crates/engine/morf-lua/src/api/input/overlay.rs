//! The overlay layer: what opens over a surface -- a menu, a dialog, a
//! tooltip -- above everything else on it.
//!
//! `morf.overlay.open(content, options)` puts `content` in its surface's
//! overlay layer, the root's last child, drawn over the rest. Overlays on a
//! surface stack: the newest is on top, and Escape and a press outside close
//! the top one first. `options`:
//!
//! - `anchor`: the node it opens beside, placed by `placement`
//!   (`"bottom-start"` by default; `top`, `bottom`, `left`, `right` or
//!   `center`, with `-start`, `-end` or neither) `gap` px away (4), flipped
//!   to the other side when its own has no room and shifted to stay `margin`
//!   px (8) inside the surface. With no anchor it is centred.
//! - `root`: the surface's root, when there is no anchor to find it from.
//! - `dim`: true, or a colour, for a scrim under it over the surface.
//! - `modal`: whether what is under it takes no input while it is open
//!   (true when it dims).
//! - `escape`, `outside`: whether Escape and a press outside it close it
//!   (both true). A press on the anchor is not outside.
//! - `focus`: whether focus moves into it as it opens (true). Closing gives
//!   focus back to the node that had it, the control that opened it --
//!   unless `restore` is false (a popup's ghost playing its exit, which
//!   must not pull focus out of whatever opened since).
//! - `on_close(reason)`: called once it has closed -- `"escape"`,
//!   `"outside"`, `"closed"` (by `morf.overlay.close`) or `"gone"` (its
//!   content was destroyed).
//!
//! Closing hides the content in the layer; opening it again shows it there.
//! `morf.overlay.track(node, options)` gives a node the layer's behaviour
//! where it already is -- a drawer: see `track`.
//! `morf.overlay.close(content)` closes one, `morf.overlay.is_open(content)`
//! asks.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use luna::{Callback, CallbackReturn, Context, Function, Table, UserRef, Value as LuaValue};
use morf_scene::overlay::Placement;
use morf_scene::{Element, NodeHandle, Value as SceneValue};

use crate::Runtime;
use crate::api_focus::FocusRequest;
use crate::scene_bindings::{assign_scene_property, create_node};
use crate::state::ReactiveState;
use crate::state_tokens::NodeToken;
use crate::types::LogLevel;
use morf_runtime::overlays::{DIM, Overlay, Overlays};

mod runtime;

/// Every surface's overlay layer and the overlays open on it; a reopen
/// waiting on its close keeps its options.
pub(crate) type OverlayState = Overlays<Option<luna::StashedTable>>;

/// A list of nodes from Lua (`except = { a, b }`), or none.
fn nodes_of<'gc>(ctx: Context<'gc>, value: LuaValue<'gc>) -> Result<Vec<NodeHandle>, String> {
    let LuaValue::Table(table) = value else {
        return Ok(Vec::new());
    };
    let mut nodes = Vec::new();
    for (_, item) in table.iter(ctx) {
        let LuaValue::UserData(data) = item else {
            return Err("overlay except must be a list of nodes".to_owned());
        };
        nodes.push(
            data.downcast_static::<NodeToken>()
                .map_err(|_| "overlay except must be a list of nodes".to_owned())?
                .handle,
        );
    }
    Ok(nodes)
}

fn set(state: &mut ReactiveState, node: NodeHandle, property: &str, value: SceneValue) {
    if let Err(message) = assign_scene_property(state, node, property, value) {
        state.log(LogLevel::Warn, format!("overlay {property}: {message}"));
    }
}

fn fill() -> SceneValue {
    SceneValue::Map(BTreeMap::from([(
        "fill".to_owned(),
        SceneValue::Bool(true),
    )]))
}

/// The surface's overlay layer, made the first time it is wanted.
fn layer_of(state: &Rc<RefCell<ReactiveState>>, root: NodeHandle) -> Result<NodeHandle, String> {
    if let Some(layer) = state.borrow().overlays.layers.get(&root).copied()
        && state.borrow().scene.contains(layer)
    {
        return Ok(layer);
    }
    let layer = create_node(state, Element::Item);
    let mut state = state.borrow_mut();
    state
        .scene
        .reparent(layer, Some(root))
        .map_err(|error| error.to_string())?;
    set(
        &mut state,
        layer,
        "id",
        SceneValue::String("morf-overlay".to_owned()),
    );
    set(&mut state, layer, "z", SceneValue::Number(1.0e6));
    state.overlays.layers.insert(root, layer);
    Ok(layer)
}

fn open<'gc>(
    ctx: Context<'gc>,
    state: &Rc<RefCell<ReactiveState>>,
    content: NodeHandle,
    options: Option<Table<'gc>>,
) -> Result<(), String> {
    let get = |key: &str| options.map_or(LuaValue::Nil, |t| t.get_value(ctx, key));
    let node = |key: &str| -> Result<Option<NodeHandle>, String> {
        match get(key) {
            LuaValue::Nil => Ok(None),
            LuaValue::UserData(data) => Ok(Some(
                data.downcast_static::<NodeToken>()
                    .map_err(|_| format!("overlay {key} must be a node"))?
                    .handle,
            )),
            _ => Err(format!("overlay {key} must be a node")),
        }
    };
    let flag = |key: &str, default: bool| match get(key) {
        LuaValue::Nil => default,
        LuaValue::Boolean(on) => on,
        _ => true,
    };
    let number = |key: &str, default: f64| match get(key) {
        LuaValue::Integer(n) => n as f64,
        LuaValue::Number(n) => n,
        _ => default,
    };
    if state.borrow().overlays.contains(content) {
        state
            .borrow_mut()
            .overlays
            .open_again(content, || options.map(|t| ctx.stash(t)));
        return Ok(());
    }
    let anchor = node("anchor")?;
    let root = match (node("root")?, anchor) {
        (Some(root), _) => state.borrow().scene.root_of(root),
        (None, Some(anchor)) => state.borrow().scene.root_of(anchor),
        (None, None) => None,
    }
    .ok_or("morf.overlay.open: give it an anchor or a root on a surface")?;
    let placement = match get("placement") {
        LuaValue::Nil => Placement::parse(if anchor.is_some() {
            "bottom-start"
        } else {
            "center"
        }),
        LuaValue::String(text) => Placement::parse(&text.display_lossy().to_string()),
        _ => None,
    }
    .ok_or("overlay placement is top, bottom, left, right or center, with -start or -end")?;
    let dim = match get("dim") {
        LuaValue::Nil | LuaValue::Boolean(false) => None,
        LuaValue::Boolean(true) => Some(SceneValue::String(DIM.to_owned())),
        value => Some(crate::reactive_bindings::lua_to_scene(ctx, value, 0)?),
    };
    let modal = flag("modal", dim.is_some());
    let on_close = match get("on_close") {
        LuaValue::Function(Function::Closure(closure)) => Some(ctx.stash(closure)),
        LuaValue::Nil => None,
        _ => return Err("overlay on_close must be a function".to_owned()),
    };
    let layer = layer_of(state, root)?;
    let existing = state.borrow().overlays.wrappers.get(&content).copied();
    let wrapper = match existing.filter(|w| state.borrow().scene.contains(*w)) {
        Some(wrapper) => {
            // Its scrim and blocker are rebuilt for these options.
            let children = state
                .borrow()
                .scene
                .children(wrapper)
                .unwrap_or_default()
                .to_vec();
            let mut s = state.borrow_mut();
            for child in children.into_iter().filter(|c| *c != content) {
                crate::runtime_helpers::remove_scene_subtree(&mut s, child);
            }
            wrapper
        }
        None => {
            let wrapper = create_node(state, Element::Item);
            state
                .borrow_mut()
                .overlays
                .wrappers
                .insert(content, wrapper);
            wrapper
        }
    };
    let blocker = modal.then(|| create_node(state, Element::MouseArea));
    let scrim = dim.as_ref().map(|_| create_node(state, Element::Rect));
    let mut s = state.borrow_mut();
    s.scene
        .reparent(wrapper, Some(layer))
        .map_err(|error| error.to_string())?;
    set(&mut s, wrapper, "anchors", fill());
    set(&mut s, wrapper, "visible", SceneValue::Bool(true));
    // Shown once it has been placed, so it never flashes at the corner.
    set(&mut s, wrapper, "opacity", SceneValue::Number(0.0));
    for (extra, color) in [(scrim, dim.clone()), (blocker, None)] {
        if let Some(extra) = extra {
            s.scene
                .reparent(extra, Some(wrapper))
                .map_err(|error| error.to_string())?;
            set(&mut s, extra, "anchors", fill());
            if let Some(color) = color {
                set(&mut s, extra, "color", color);
            }
        }
    }
    crate::runtime_helpers::cancel_node_exit(&mut s, content);
    s.scene
        .reparent(content, Some(wrapper))
        .map_err(|error| error.to_string())?;
    let restore = s.focus.owner.get(&root).copied().map(|node| {
        (
            node,
            s.scene.bool_value(node, "visual_focus").unwrap_or(false),
        )
    });
    if flag("focus", true) {
        s.focus
            .requests
            .push(FocusRequest::Into(content, restore.is_some_and(|(_, v)| v)));
    }
    s.overlays.stack.push(Overlay {
        root,
        wrapper,
        content,
        anchor,
        placement,
        gap: number("gap", 4.0),
        margin: number("margin", 8.0),
        modal,
        escape: flag("escape", true),
        outside: flag("outside", true),
        on_close: on_close.map(crate::vm::handler_store::register),
        restore,
        give_back: flag("restore", true),
        placed: None,
        tracked: false,
        except: nodes_of(ctx, get("except"))?,
    });
    Ok(())
}

/// `morf.overlay.track(node, options)`: an overlay that stays where it is --
/// a drawer, a panel the configuration placed itself -- with the layer's
/// behaviour: the stack, Escape, a press outside it (a catcher behind it
/// takes presses anywhere else on its surface while it is open), focus in
/// and back. `options` as `open`'s, less the placement.
fn track<'gc>(
    ctx: Context<'gc>,
    state: &Rc<RefCell<ReactiveState>>,
    content: NodeHandle,
    options: Option<Table<'gc>>,
) -> Result<(), String> {
    let get = |key: &str| options.map_or(LuaValue::Nil, |t| t.get_value(ctx, key));
    let flag = |key: &str, default: bool| match get(key) {
        LuaValue::Nil => default,
        LuaValue::Boolean(on) => on,
        _ => true,
    };
    if state.borrow().overlays.contains(content) {
        return Ok(());
    }
    let anchor = match get("anchor") {
        LuaValue::UserData(data) => Some(
            data.downcast_static::<NodeToken>()
                .map_err(|_| "overlay anchor must be a node".to_owned())?
                .handle,
        ),
        _ => None,
    };
    let root = state
        .borrow()
        .scene
        .root_of(content)
        .ok_or("morf.overlay.track: the node is on no surface")?;
    let on_close = match get("on_close") {
        LuaValue::Function(Function::Closure(closure)) => Some(ctx.stash(closure)),
        LuaValue::Nil => None,
        _ => return Err("overlay on_close must be a function".to_owned()),
    };
    let outside = flag("outside", true);
    let catcher = create_node(state, Element::MouseArea);
    let mut s = state.borrow_mut();
    s.scene
        .reparent(catcher, Some(root))
        .map_err(|error| error.to_string())?;
    set(&mut s, catcher, "anchors", fill());
    // Behind everything on the surface: presses on the rest of the shell
    // still reach it; presses on nothing reach the catcher.
    set(&mut s, catcher, "z", SceneValue::Number(-1.0e6));
    set(&mut s, catcher, "visible", SceneValue::Bool(outside));
    set(
        &mut s,
        catcher,
        "id",
        SceneValue::String("morf-overlay-catcher".to_owned()),
    );
    let restore = s.focus.owner.get(&root).copied().map(|node| {
        (
            node,
            s.scene.bool_value(node, "visual_focus").unwrap_or(false),
        )
    });
    if flag("focus", false) {
        s.focus
            .requests
            .push(FocusRequest::Into(content, restore.is_some_and(|(_, v)| v)));
    }
    s.overlays.stack.push(Overlay {
        root,
        wrapper: catcher,
        content,
        anchor,
        placement: Placement::parse("center").expect("a placement"),
        gap: 0.0,
        margin: 0.0,
        modal: flag("modal", false),
        escape: flag("escape", true),
        outside,
        on_close: on_close.map(crate::vm::handler_store::register),
        restore,
        give_back: flag("restore", true),
        placed: None,
        tracked: true,
        except: nodes_of(ctx, get("except"))?,
    });
    Ok(())
}

/// `morf.overlay`: `open(content, options)`, `close(content)`,
/// `is_open(content)`.
pub(crate) fn install_overlay_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let overlay = Table::new(&ctx);
    let opener = Rc::clone(&state);
    overlay.set_field(
        ctx,
        "open",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (content, options): (UserRef<NodeToken>, Option<Table>) = stack.consume(ctx)?;
            open(ctx, &opener, content.handle, options)
                .map_err(crate::scene_bindings::HostError)?;
            Ok(CallbackReturn::Return)
        }),
    );
    let tracker = Rc::clone(&state);
    overlay.set_field(
        ctx,
        "track",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (content, options): (UserRef<NodeToken>, Option<Table>) = stack.consume(ctx)?;
            track(ctx, &tracker, content.handle, options)
                .map_err(crate::scene_bindings::HostError)?;
            Ok(CallbackReturn::Return)
        }),
    );
    let closer = Rc::clone(&state);
    overlay.set_field(
        ctx,
        "close",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let content: UserRef<NodeToken> = stack.consume(ctx)?;
            closer
                .borrow_mut()
                .overlays
                .request_close(content.handle, "closed");
            Ok(CallbackReturn::Return)
        }),
    );
    let asker = Rc::clone(&state);
    overlay.set_field(
        ctx,
        "is_open",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let content: UserRef<NodeToken> = stack.consume(ctx)?;
            let open = asker.borrow().overlays.is_open(content.handle);
            stack.replace(ctx, open);
            Ok(CallbackReturn::Return)
        }),
    );
    morf.set_field(ctx, "overlay", overlay);
}
