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
//!   focus back to the node that had it, the control that opened it.
//! - `on_close(reason)`: called once it has closed -- `"escape"`,
//!   `"outside"`, `"closed"` (by `morf.overlay.close`) or `"gone"` (its
//!   content was destroyed).
//!
//! Closing hides the content in the layer; opening it again shows it there.
//! `morf.overlay.close(content)` closes one, `morf.overlay.is_open(content)`
//! asks.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use luna::{
    Callback, CallbackReturn, Context, Function, StashedClosure, Table, UserRef, Value as LuaValue,
};
use morf_layout::{Geometry, Layout};
use morf_scene::overlay::{Bounds, Placement, place};
use morf_scene::{Element, NodeHandle, Value as SceneValue};

use crate::IpcValue;
use crate::Runtime;
use crate::api_focus::{FocusReason, FocusRequest};
use crate::reactive_execute::execute_ipc_handler;
use crate::scene_bindings::{assign_scene_property, create_node};
use crate::state::ReactiveState;
use crate::state_tokens::NodeToken;
use crate::types::LogLevel;

const DIM: &str = "#00000052";

pub(crate) struct Overlay {
    root: NodeHandle,
    wrapper: NodeHandle,
    content: NodeHandle,
    anchor: Option<NodeHandle>,
    placement: Placement,
    gap: f64,
    margin: f64,
    modal: bool,
    escape: bool,
    outside: bool,
    on_close: Option<StashedClosure>,
    /// The node that had focus when it opened, and whether it showed it.
    restore: Option<(NodeHandle, bool)>,
    placed: Option<(f64, f64)>,
}

/// Every surface's overlay layer and the overlays open on it.
#[derive(Default)]
pub(crate) struct OverlayState {
    layers: HashMap<NodeHandle, NodeHandle>,
    /// The wrapper each content was put in, kept while it is closed.
    wrappers: HashMap<NodeHandle, NodeHandle>,
    pub(crate) stack: Vec<Overlay>,
    closing: Vec<(NodeHandle, &'static str)>,
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

fn within(state: &ReactiveState, outer: NodeHandle, node: NodeHandle) -> bool {
    crate::runtime_helpers::scene_node_in_subtree(&state.scene, outer, node)
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
    if state
        .borrow()
        .overlays
        .stack
        .iter()
        .any(|o| o.content == content)
    {
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
        on_close,
        restore,
        placed: None,
    });
    Ok(())
}

impl Runtime {
    /// Places every open overlay against the layout just drawn: the layers
    /// sized to their surfaces, each overlay beside its anchor.
    pub(crate) fn place_overlays(&mut self, layout: &Layout) {
        let mut state = self.reactive.borrow_mut();
        if state.overlays.stack.is_empty() {
            return;
        }
        let layers: Vec<(NodeHandle, NodeHandle)> = state
            .overlays
            .layers
            .iter()
            .map(|(r, l)| (*r, *l))
            .collect();
        for (root, layer) in layers {
            if let Some(g) = layout.geometry(root) {
                for (property, value) in [("width", g.width), ("height", g.height)] {
                    if state.scene.number(layer, property).ok() != Some(value) {
                        set(&mut state, layer, property, SceneValue::Number(value));
                    }
                }
            }
        }
        for index in 0..state.overlays.stack.len() {
            let overlay = &state.overlays.stack[index];
            let (root, content, anchor) = (overlay.root, overlay.content, overlay.anchor);
            let (Some(surface), Some(size)) = (layout.geometry(root), layout.geometry(content))
            else {
                continue;
            };
            let anchor = anchor.and_then(|anchor| {
                let g = state.transform_tracker.geometry(anchor)?;
                let rect = Geometry {
                    x: 0.0,
                    y: 0.0,
                    width: g.width,
                    height: g.height,
                };
                state
                    .transform_tracker
                    .map_rect_from_node(&state.scene, anchor, rect)
                    .ok()
                    .flatten()
            });
            let overlay = &state.overlays.stack[index];
            let ((x, y), _) = place(
                anchor.map(|a| Bounds {
                    x: a.x,
                    y: a.y,
                    width: a.width,
                    height: a.height,
                }),
                (size.width, size.height),
                Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: surface.width,
                    height: surface.height,
                },
                overlay.placement,
                overlay.gap,
                overlay.margin,
            );
            let (x, y) = (x.round(), y.round());
            if overlay.placed != Some((x, y)) {
                let wrapper = overlay.wrapper;
                state.overlays.stack[index].placed = Some((x, y));
                set(&mut state, content, "x", SceneValue::Number(x));
                set(&mut state, content, "y", SceneValue::Number(y));
                set(&mut state, wrapper, "opacity", SceneValue::Number(1.0));
            }
        }
    }

    /// The roots of the surfaces something is open over, for a host to match
    /// a press against the surface it landed on.
    pub fn overlay_roots(&self) -> Vec<NodeHandle> {
        let mut roots: Vec<NodeHandle> = self
            .reactive
            .borrow()
            .overlays
            .stack
            .iter()
            .map(|o| o.root)
            .collect();
        roots.dedup();
        roots
    }

    /// The top overlay open on the surface whose tree is `root`.
    fn top_overlay(&self, root: NodeHandle) -> Option<usize> {
        self.reactive
            .borrow()
            .overlays
            .stack
            .iter()
            .rposition(|o| o.root == root)
    }

    /// Escape on a surface: closes its top overlay if Escape may. Returns
    /// whether it did.
    pub fn overlay_escape(&mut self, root: NodeHandle) -> bool {
        match self.top_overlay(root) {
            Some(index) if self.reactive.borrow().overlays.stack[index].escape => {
                self.close_overlay(index, "escape");
                true
            }
            _ => false,
        }
    }

    /// A press on a surface, on `hit`: closes its top overlay when the press
    /// is outside it (and not on its anchor) and it closes so. Returns
    /// whether it closed one.
    pub fn overlay_press(&mut self, root: NodeHandle, hit: Option<NodeHandle>) -> bool {
        let Some(index) = self.top_overlay(root) else {
            return false;
        };
        let outside = {
            let state = self.reactive.borrow();
            let overlay = &state.overlays.stack[index];
            overlay.outside
                && !hit.is_some_and(|hit| {
                    within(&state, overlay.content, hit)
                        || overlay
                            .anchor
                            .is_some_and(|anchor| within(&state, anchor, hit))
                })
        };
        if outside {
            self.close_overlay(index, "outside");
        }
        outside
    }

    /// What Tab walks on a surface: inside a modal overlay while one is
    /// open, the whole tree otherwise.
    pub fn focus_root(&self, root: NodeHandle) -> NodeHandle {
        let state = self.reactive.borrow();
        state
            .overlays
            .stack
            .iter()
            .rev()
            .find(|o| o.root == root && o.modal)
            .map_or(root, |o| o.content)
    }

    fn close_overlay(&mut self, index: usize, reason: &'static str) {
        let overlay = self.reactive.borrow_mut().overlays.stack.remove(index);
        let refocus = {
            let mut state = self.reactive.borrow_mut();
            if state.scene.contains(overlay.wrapper) {
                set(
                    &mut state,
                    overlay.wrapper,
                    "visible",
                    SceneValue::Bool(false),
                );
            }
            let owner = state.focus.owner.get(&overlay.root).copied();
            owner.is_none_or(|owner| {
                !state.scene.contains(owner) || within(&state, overlay.content, owner)
            })
        };
        if refocus {
            let (node, visual) = overlay.restore.unzip();
            let node = node.filter(|node| self.reactive.borrow().scene.can_hold_focus(*node));
            let reason = if visual == Some(true) {
                FocusReason::Keyboard
            } else {
                FocusReason::Program
            };
            self.set_focus(overlay.root, node, reason);
        }
        if let Some(on_close) = overlay.on_close {
            let args = [IpcValue::String(reason.to_owned())];
            if let Err(message) =
                self.run_handler(|ctx, limits| execute_ipc_handler(ctx, &on_close, &args, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("overlay on_close: {message}"));
            }
        }
    }

    /// Closes what `morf.overlay.close` asked to, and any overlay whose
    /// content is gone. Returns whether one closed.
    pub(crate) fn poll_overlays(&mut self) -> bool {
        let mut closing = std::mem::take(&mut self.reactive.borrow_mut().overlays.closing);
        {
            let state = self.reactive.borrow();
            for overlay in &state.overlays.stack {
                if !state.scene.contains(overlay.content) {
                    closing.push((overlay.content, "gone"));
                }
            }
        }
        let mut closed = false;
        for (content, reason) in closing {
            let index = self
                .reactive
                .borrow()
                .overlays
                .stack
                .iter()
                .position(|o| o.content == content);
            if let Some(index) = index {
                self.close_overlay(index, reason);
                closed = true;
            }
        }
        closed
    }
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
    let closer = Rc::clone(&state);
    overlay.set_field(
        ctx,
        "close",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let content: UserRef<NodeToken> = stack.consume(ctx)?;
            closer
                .borrow_mut()
                .overlays
                .closing
                .push((content.handle, "closed"));
            Ok(CallbackReturn::Return)
        }),
    );
    let asker = Rc::clone(&state);
    overlay.set_field(
        ctx,
        "is_open",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let content: UserRef<NodeToken> = stack.consume(ctx)?;
            let open = {
                let state = asker.borrow();
                state
                    .overlays
                    .stack
                    .iter()
                    .any(|o| o.content == content.handle)
                    && !state
                        .overlays
                        .closing
                        .iter()
                        .any(|(c, _)| *c == content.handle)
            };
            stack.replace(ctx, open);
            Ok(CallbackReturn::Return)
        }),
    );
    morf.set_field(ctx, "overlay", overlay);
}
