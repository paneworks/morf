//! What a frame's layout tells the configuration.
//!
//! Split from the service loop at the line gate. Once a surface is laid
//! out, the resolved geometry feeds transform watchers, popup anchors, and
//! the bindings that read `layout_x` and its kin.

use morf_layout::{Layout, Size, TextMeasurer};
use morf_scene::NodeHandle;

use crate::{
    reactive_bindings::*, runtime_helpers::*, scene_bindings::*, surface_types::*, types::*,
};

/// A layout that has stopped moving, and how many passes it took.
pub struct SettledLayout {
    pub layout: Layout,
    /// Layout passes run, one or more.
    pub passes: usize,
    /// Whether the last pass changed nothing a layout reads. False when the
    /// passes ran out first: a binding that feeds its own geometry back.
    pub stable: bool,
}

impl Runtime {
    /// Lays `root` out until the bindings that read the layout agree with it.
    ///
    /// A binding on `layout_width` and its kin hears about a frame only after
    /// the frame: the first layout moves a node, [`Runtime::observe_layout`]
    /// flushes the bindings that read it, and those may move something else.
    /// A running shell converges over successive frames; a one-shot render
    /// (a benchmark, a picture) has to do the same passes up front or it
    /// draws the first, misplaced one. Stops once a pass leaves the scene's
    /// layout revision where it was, or after `max_passes` (at least one).
    pub fn settle_layout(
        &mut self,
        root: NodeHandle,
        available: Size,
        text: &mut impl TextMeasurer,
        max_passes: usize,
    ) -> Result<SettledLayout, String> {
        let max_passes = max_passes.max(1);
        let mut passes = 0;
        loop {
            let layout = self.compute_layout(root, available, text)?;
            passes += 1;
            let before = self.scene().layout_revision_of(root);
            self.observe_layout(&layout);
            let stable = self.scene().layout_revision_of(root) == before;
            if stable || passes >= max_passes {
                return Ok(SettledLayout {
                    layout,
                    passes,
                    stable,
                });
            }
        }
    }

    /// Tells every stretching node in a frame's layout where it is, so its
    /// spring steps before the frame is painted.
    ///
    /// Between layout and paint, on every frame — not only a fresh layout's:
    /// a node sliding on `translate_x` moves without its layout changing.
    /// Free when nothing in the scene stretches.
    pub fn observe_stretch(&mut self, layout: &Layout) {
        let mut state = self.reactive.borrow_mut();
        if !state.scene.has_stretch() {
            return;
        }
        if let Err(error) = morf_layout::observe_stretch(&mut state.scene, layout) {
            state.log(LogLevel::Warn, format!("stretch: {error}"));
        }
    }

    /// Updates native transform watchers from one rendered surface layout.
    ///
    /// Also where a binding on `layout_width` and its kin hears that the
    /// frame moved its node: every node such a binding read is checked
    /// against the geometry it had, and the changed ones are flushed here,
    /// since nothing else would until the next event.
    pub fn observe_layout(&mut self, layout: &Layout) -> bool {
        let moved = {
            let state = self.reactive.borrow();
            state
                .property_signals
                .keys()
                .filter_map(|(node, property, _)| {
                    let size = match property.as_str() {
                        LAYOUT_SIZE => true,
                        LAYOUT_POSITION => false,
                        _ => return None,
                    };
                    let now = layout.geometry(*node)?;
                    let before = state.transform_tracker.geometry(*node);
                    let changed = match before {
                        None => true,
                        Some(before) if size => {
                            before.width != now.width || before.height != now.height
                        }
                        Some(before) => before.x != now.x || before.y != now.y,
                    };
                    changed.then_some((*node, if size { LAYOUT_SIZE } else { LAYOUT_POSITION }))
                })
                .collect::<Vec<_>>()
        };
        let mut state = self.reactive.borrow_mut();
        state.transform_tracker.update(layout);
        for (node, which) in &moved {
            let _ = bump_property_signal(&mut state, *node, which, false);
        }
        drop(state);
        if !moved.is_empty()
            && let Err(message) = self
                .lua
                .enter(|ctx| flush_reactive(&self.reactive, ctx, self.limits))
        {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("layout binding: {message}"));
        }
        let mut state = self.reactive.borrow_mut();
        let anchors = state
            .popup_node_anchors
            .iter()
            .map(|(id, anchor)| (*id, anchor.clone()))
            .collect::<Vec<_>>();
        for (id, anchor) in anchors {
            let Some(geometry) = state.transform_tracker.geometry(anchor.node) else {
                continue;
            };
            let node_width = geometry_i32(geometry.width).max(1);
            let node_height = geometry_i32(geometry.height).max(1);
            let resolved = (
                geometry_i32(geometry.x)
                    .saturating_add(anchor.x)
                    .saturating_sub(anchor.margin_left),
                geometry_i32(geometry.y)
                    .saturating_add(anchor.y)
                    .saturating_sub(anchor.margin_top),
                anchor
                    .width
                    .unwrap_or(node_width)
                    .saturating_add(anchor.margin_left)
                    .saturating_add(anchor.margin_right)
                    .max(1),
                anchor
                    .height
                    .unwrap_or(node_height)
                    .saturating_add(anchor.margin_top)
                    .saturating_add(anchor.margin_bottom)
                    .max(1),
            );
            if let Some(WindowSurfaceConfig {
                kind: WindowSurfaceKind::Popup(config),
                ..
            }) = state.window_surfaces.get_mut(&id)
                && (
                    config.anchor_x,
                    config.anchor_y,
                    config.anchor_width,
                    config.anchor_height,
                ) != resolved
            {
                config.anchor_x = resolved.0;
                config.anchor_y = resolved.1;
                config.anchor_width = resolved.2;
                config.anchor_height = resolved.3;
                state.window_surfaces_changed = true;
            }
        }
        let mut watchers = std::mem::take(&mut state.transform_watchers);
        let mut changed = false;
        for watcher in watchers.values_mut() {
            match watcher
                .watcher
                .observe(&state.scene, &state.transform_tracker)
            {
                Ok(true) => {
                    watcher.revision = watcher.revision.wrapping_add(1);
                    watcher.pending = true;
                    changed = true;
                }
                Ok(false) => {}
                Err(error) => state.log(LogLevel::Warn, format!("transform watcher: {error}")),
            }
        }
        state.transform_watchers = watchers;
        changed
    }
}
