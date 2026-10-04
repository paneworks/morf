//! The paint half of a turn: whether this turn paints, and the paint.

use morf_app::{PRIMARY_LAYER, WindowId};
use morf_lua::Runtime;
use std::time::Instant;

use crate::host::windows::Kind;
use crate::painter::Painter;
use crate::surface_actions::apply_parent_transitions;
use crate::{paint::*, surface_pointer::answer_new_containment, surfaces::*};

use super::{Host, Turn, complain_layout, is_layout_error, slow};

impl Host {
    /// Paints what changed (`repaint`, or motion with nothing else to drive
    /// it, or a paint owed past a stall), then answers what the fresh
    /// layouts made answerable: new `contains_pointer` questions and a
    /// screen reader's tree.
    pub(super) fn frame(
        &mut self,
        runtime: &mut Runtime,
        mut repaint: bool,
    ) -> Result<Turn, String> {
        let name = self.name.clone();
        let frame_wait = self.backend.layer_frame_wait(PRIMARY_LAYER);
        // Motion with nothing to drive it: started where no turn noticed (a
        // binding flushed after an animation's `on_finished`, say) while no
        // frame callback is outstanding. The callbacks are its clock, and
        // only a paint asks for one -- without this it waits, frozen, for
        // whatever paints next: a pill that stays lit, a swell that shows
        // seconds late.
        if !repaint && frame_wait.is_none() && runtime.has_motion() {
            repaint = true;
        }
        // A paint owed for longer than a stall is made without the callback.
        let owed = owed_paint_due(
            self.state.primary_deferred,
            frame_wait,
            self.state.refresh,
            self.state.forced_paint,
            Instant::now(),
        );
        if owed {
            self.state.primary_deferred = false;
            self.state.forced_paint = Some(Instant::now());
            repaint = true;
        }
        // A surface still waiting for its last frame callback is not
        // presented to again: under FIFO the present blocks until that
        // callback, and a surface the compositor is not showing (a fallback
        // toplevel under another, in cage) never gets one -- which froze this
        // whole output, every other surface and IPC with it. The callback,
        // when it comes, makes the paint.
        if repaint && !owed && frame_wait.is_some() {
            self.state.primary_deferred = true;
            let state = &mut self.state;
            for (_, surface) in state.windows.of_kind_mut(Kind::Layer) {
                surface.needs_paint |= surface.updates_enabled;
            }
            repaint = false;
            // The layer surfaces are not held by the primary's callback;
            // each paints when its own allows.
            for (_, surface) in state.windows.of_kind_mut(Kind::Layer) {
                if surface.updates_enabled {
                    paint_layer_surface(
                        runtime,
                        &*self.backend,
                        surface,
                        state.painter.layout_only(),
                    )?;
                }
            }
        }
        if repaint {
            match self.paint(runtime)? {
                Turn::Again => {}
                other => return Ok(other),
            }
        }
        // Layout observation can rebuild a responsive authentication tree
        // during paint. Draw that new tree before answering its pointer
        // watchers, otherwise the first pointer position is consumed against
        // removed nodes and monitor ownership stays wrong until the next move.
        //
        // A paint held back for the frame callback cannot happen this turn,
        // so the tree stays newer than the layout until the callback comes:
        // turning again at once spun the loop for the whole wait (10-40 ms
        // a frame on a busy GPU), thousands of turns a second while anything
        // ticked. The paint is owed instead; the callback, or the stall
        // deadline when none comes, makes it.
        if runtime.scene().layout_revision_of(self.state.primary_root) != self.state.layout.revision
        {
            self.containment_repaint = true;
            if self.backend.layer_frame_wait(PRIMARY_LAYER).is_some() {
                self.state.primary_deferred = true;
            } else {
                self.follow_up = true;
            }
            return Ok(Turn::Again);
        }
        // After the paints, so a node built this turn is laid out by now.
        let layouts = LayerLayouts {
            layout: &self.state.layout,
            windows: &self.state.windows,
        };
        if answer_new_containment(runtime, &self.state.input, &layouts) {
            self.containment_repaint = true;
            self.follow_up = true;
        }
        // A screen reader's tree and requests, once the layouts are fresh.
        for (root, focused) in self.state.keyboard_changes.drain(..) {
            self.a11y.window_focus(root, focused);
        }
        if self.a11y.turn(runtime, &self.state, &name, repaint) {
            self.follow_up = true;
        }
        Ok(Turn::Again)
    }

    /// Paints the shell's own surface and every window that takes updates.
    fn paint(&mut self, runtime: &mut Runtime) -> Result<Turn, String> {
        let painted = Instant::now();
        let state = &mut self.state;
        let client = &*self.backend;
        // Before anything is drawn, tell the renderer what died. Its caches
        // are keyed on nodes and it has no other way to find out; without
        // this a shaped text buffer survives every view switch for the life
        // of the process.
        let removed = runtime.take_removed_nodes();
        if let Painter::Gpu(renderer) = &mut state.painter {
            renderer
                .backend_mut()
                .set_elapsed(self.started.elapsed().as_secs_f32());
            if !removed.is_empty() {
                renderer.backend_mut().forget_nodes(&removed);
            }
        }
        if !removed.is_empty() {
            for renderer in state
                .windows
                .values_mut()
                .filter_map(|surface| surface.renderer.as_mut())
            {
                renderer.backend_mut().forget_nodes(&removed);
            }
        }
        apply_parent_transitions(runtime, &mut state.painter, client)?;
        let painting = Instant::now();
        let painted_frame = paint(
            runtime,
            &mut state.painter,
            client,
            state.primary_root,
            Some(&mut state.layout),
        );
        slow(self.report_slow, &self.name, "a frame", painting);
        // Skipped for want of a buffer: owed, and painted on the next
        // callback (or when the callback is overdue).
        if let Painter::Gpu(renderer) = &mut state.painter
            && renderer.backend_mut().take_skipped()
        {
            state.primary_deferred = true;
            // Nothing committed, so no callback may be coming: ask for one,
            // or the owed paint waits for whatever paints next.
            if client.layer_frame_wait(PRIMARY_LAYER).is_none() {
                client.request_frame(WindowId::Layer(PRIMARY_LAYER));
                client.commit(WindowId::Layer(PRIMARY_LAYER));
            }
        }
        match painted_frame {
            Ok(layout) => state.layout = layout,
            Err(error) if is_layout_error(&error) => {
                complain_layout(&self.name, &error, &mut self.layout_complaint);
                return Ok(Turn::Again);
            }
            Err(error) => return Err(error),
        }
        for (_, surface) in state.windows.of_kind_mut(Kind::Popup) {
            if surface.updates_enabled {
                paint_popup_surface(runtime, client, surface, state.painter.layout_only())?;
            }
        }
        for (_, surface) in state.windows.of_kind_mut(Kind::Toplevel) {
            if surface.updates_enabled {
                paint_floating_surface(runtime, client, surface, state.painter.layout_only())?;
            }
        }
        for (_, surface) in state.windows.of_kind_mut(Kind::Layer) {
            if surface.updates_enabled {
                paint_layer_surface(runtime, client, surface, state.painter.layout_only())?;
            }
        }
        // What this frame actually cost, which is what the next one is paced
        // against.
        state.pacer.observed(painted.elapsed());
        Ok(Turn::Again)
    }
}
