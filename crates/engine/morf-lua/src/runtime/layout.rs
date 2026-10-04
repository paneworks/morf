//! What a frame's layout tells the configuration.
//!
//! Split from the service loop at the line gate. Once a surface is laid
//! out, the resolved geometry feeds transform watchers, popup anchors, and
//! the bindings that read `layout_x` and its kin.

use morf_layout::{Layout, Size, TextMeasurer};
use morf_scene::NodeHandle;

use crate::{reactive_bindings::*, scene_bindings::*, types::*};

pub use morf_runtime::layout::SettledLayout;

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
        self.observe_layout_with(layout, true)
    }

    /// Observes a rendered layout, reusing its geometry when the host reused
    /// the layout unchanged. Transform watchers still observe the scene on
    /// every frame: translation, rotation and stretch do not require layout.
    pub fn observe_layout_with(&mut self, layout: &Layout, geometry_changed: bool) -> bool {
        let moved = if geometry_changed {
            let state = self.reactive.borrow();
            morf_runtime::layout::moved_nodes(
                state
                    .property_signals
                    .keys()
                    .map(|(node, property, _)| (*node, property.as_str())),
                layout,
                &state.transform_tracker,
            )
        } else {
            Vec::new()
        };
        let mut state = self.reactive.borrow_mut();
        if geometry_changed {
            state.transform_tracker.update(layout);
        }
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
        self.place_overlays(layout);
        let mut state = self.reactive.borrow_mut();
        let state = &mut *state;
        if morf_runtime::layout::place_popup_anchors(
            &state.windows.popup_node_anchors,
            &state.transform_tracker,
            &mut state.windows.window_surfaces,
        ) {
            state.windows.window_surfaces_changed = true;
        }
        let (changed, errors) = morf_runtime::layout::observe_transform_watches(
            &mut state.transform_watchers,
            &state.scene,
            &state.transform_tracker,
        );
        for error in errors {
            state.log(LogLevel::Warn, error);
        }
        changed
    }
}
