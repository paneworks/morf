//! The overlays from the runtime's side: placed after layout, closed by
//! Escape and a press outside, focus kept inside them, and the ones whose
//! content went noticed each turn.

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

    /// The nodes whose boxes decide whether a press on `root` is outside its
    /// overlays: their contents and anchors.
    pub fn overlay_nodes(&self, root: NodeHandle) -> Vec<NodeHandle> {
        let state = self.reactive.borrow();
        state
            .overlays
            .stack
            .iter()
            .filter(|o| o.root == root)
            .flat_map(|o| {
                std::iter::once(o.content)
                    .chain(o.anchor)
                    .chain(o.except.iter().copied())
            })
            .collect()
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
        let Some(index) = self.top_overlay(root) else {
            return false;
        };
        let outside = {
            let state = self.reactive.borrow();
            let overlay = &state.overlays.stack[index];
            overlay.outside
                && !inside.contains(&overlay.content)
                && !overlay
                    .anchor
                    .is_some_and(|anchor| inside.contains(&anchor))
                && !overlay.except.iter().any(|node| inside.contains(node))
                && !hit.is_some_and(|hit| {
                    within(&state, overlay.content, hit)
                        || overlay
                            .anchor
                            .is_some_and(|anchor| within(&state, anchor, hit))
                        || overlay.except.iter().any(|node| within(&state, *node, hit))
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
            owner.is_none_or(|owner| {
                !state.scene.contains(owner) || within(&state, overlay.content, owner)
            })
        };
        if refocus && overlay.give_back {
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
