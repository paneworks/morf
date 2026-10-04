//! The overlays from the runtime's side: placed after layout, closed by
//! Escape and a press outside, focus kept inside them, and the ones whose
//! content went noticed each turn. What happens to the stack is
//! `morf_runtime::overlays`'; this does it to the scene.

use morf_layout::{Geometry, Layout};
use morf_scene::overlay::Bounds;

use super::*;

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
            if overlay.tracked {
                continue;
            }
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
            let overlay = &mut state.overlays.stack[index];
            let moved = overlay.place(
                (surface.width, surface.height),
                (size.width, size.height),
                anchor.map(|a| Bounds {
                    x: a.x,
                    y: a.y,
                    width: a.width,
                    height: a.height,
                }),
            );
            if let Some((x, y)) = moved {
                let wrapper = overlay.wrapper;
                set(&mut state, content, "x", SceneValue::Number(x));
                set(&mut state, content, "y", SceneValue::Number(y));
                set(&mut state, wrapper, "opacity", SceneValue::Number(1.0));
            }
        }
    }

    /// The roots of the surfaces something is open over, for a host to match
    /// a press against the surface it landed on.
    pub fn overlay_roots(&self) -> Vec<NodeHandle> {
        self.reactive.borrow().overlays.roots()
    }

    /// The nodes whose boxes decide whether a press on `root` is outside its
    /// overlays: their contents and anchors.
    pub fn overlay_nodes(&self, root: NodeHandle) -> Vec<NodeHandle> {
        self.reactive.borrow().overlays.nodes(root)
    }

    /// Escape on a surface: closes its top overlay if Escape may. Returns
    /// whether it did.
    pub fn overlay_escape(&mut self, root: NodeHandle) -> bool {
        let closed = self.reactive.borrow_mut().overlays.escape(root);
        closed
            .map(|overlay| self.close_overlay(overlay, "escape"))
            .is_some()
    }

    /// A press on a surface, on `hit`, at a point within the boxes of
    /// `inside` (of the nodes `overlay_nodes` named): closes its top
    /// overlay when the press is outside it (and not on its anchor) and it
    /// closes so. Returns whether it closed one.
    pub fn overlay_press(
        &mut self,
        root: NodeHandle,
        hit: Option<NodeHandle>,
        inside: &[NodeHandle],
    ) -> bool {
        let closed = {
            let mut state = self.reactive.borrow_mut();
            let state = &mut *state;
            state.overlays.press(&state.engine.scene, root, hit, inside)
        };
        closed
            .map(|overlay| self.close_overlay(overlay, "outside"))
            .is_some()
    }

    /// What Tab walks on a surface: inside a modal overlay while one is
    /// open, the whole tree otherwise.
    pub fn focus_root(&self, root: NodeHandle) -> NodeHandle {
        self.reactive.borrow().overlays.focus_root(root)
    }

    /// Does to the scene what closing `overlay`, already off the stack,
    /// means; gives focus back and tells its `on_close`.
    fn close_overlay(&mut self, overlay: Overlay, reason: &'static str) {
        let back = {
            let mut state = self.reactive.borrow_mut();
            if overlay.tracked {
                // The catcher goes; the node stays, its owner shuts it.
                crate::runtime_helpers::remove_scene_subtree(&mut state, overlay.wrapper);
            } else if state.scene.contains(overlay.wrapper) {
                set(
                    &mut state,
                    overlay.wrapper,
                    "visible",
                    SceneValue::Bool(false),
                );
            }
            let owner = state.focus.owner.get(&overlay.root).copied();
            overlay.give_focus_back(&state.scene, owner, |node| state.scene.can_hold_focus(node))
        };
        if let Some((node, reason)) = back {
            self.set_focus(overlay.root, node, reason);
        }
        if let Err(message) = overlay.notify_closed(self, reason) {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("overlay on_close: {message}"));
        }
    }

    /// Closes what `morf.overlay.close` asked to, and any overlay whose
    /// content is gone. Returns whether one closed.
    pub(crate) fn poll_overlays(&mut self) -> bool {
        let closing = {
            let mut state = self.reactive.borrow_mut();
            let state = &mut *state;
            state
                .overlays
                .take_closing(|content| state.engine.scene.contains(content))
        };
        let mut closed = false;
        for (content, reason) in closing {
            let overlay = self.reactive.borrow_mut().overlays.remove(content);
            if let Some(overlay) = overlay {
                self.close_overlay(overlay, reason);
                closed = true;
            }
        }
        let reopening = std::mem::take(&mut self.reactive.borrow_mut().overlays.reopening);
        for (content, options) in reopening {
            let state = Rc::clone(&self.reactive);
            let result = self.lua.enter(|ctx| {
                let options = options.as_ref().map(|t| ctx.fetch(t));
                open(ctx, &state, content, options)
            });
            if let Err(message) = result {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("overlay reopen: {message}"));
            }
            closed = true;
        }
        closed
    }
}
