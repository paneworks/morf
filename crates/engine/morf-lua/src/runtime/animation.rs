use morf_scene::NodeHandle;
use std::time::Duration;

use morf_scene::AnimationFrame;

use crate::vm::types::*;

// The animation frame tick and the Lua handlers it reports completions to.

impl Runtime {
    /// Advances animations entirely in Rust and reports the ones that ended.
    ///
    /// The tick itself runs no Lua. Only the `on_finished` handlers declared on
    /// behaviors are invoked afterwards, and a failing one is logged rather than
    /// allowed to abort the frame.
    /// Whether anything is in motion, so a loop standing in for the
    /// compositor's frame callbacks knows whether to keep ticking.
    /// What is animating right now, one line each (`path.property`, marked
    /// when it loops forever), at most `max`: for `MORF_LOG=wake`, which asks
    /// what keeps an otherwise idle shell drawing frames.
    pub fn motion_report(&self, max: usize) -> Vec<String> {
        self.reactive.borrow().motion_report(max)
    }

    /// What changed for layout under `root` since the layout at revision
    /// `since`: the nodes stamped since, one line each, at most `max`, and
    /// how many there were. For `MORF_PROFILE=frame_split`, when a layout is slow.
    pub fn layout_report(&self, root: NodeHandle, since: u64, max: usize) -> (usize, Vec<String>) {
        self.reactive.borrow().layout_report(root, since, max)
    }

    pub fn has_motion(&self) -> bool {
        self.reactive.borrow().has_motion()
    }

    /// Tells the runtime whether the host animates something of its own (a
    /// surface turning), which Loader preloads wait for as for motion.
    pub fn set_host_motion(&mut self, moving: bool) {
        let mut state = self.reactive.borrow_mut();
        if state.retained().host_motion != moving {
            state.retained_mut().host_motion = moving;
        }
    }

    /// Moves every theme colour easing to a new value on by `delta`, and
    /// hands each reader the colour on show.
    /// Returns how many colours moved.
    fn advance_theme_fades(&mut self, delta: Duration) -> usize {
        let moved = self.reactive.borrow_mut().advance_theme_fades(delta);
        if moved == 0 {
            return 0;
        }
        let limits = self.limits;
        let reactive = std::rc::Rc::clone(&self.reactive);
        self.lua.enter(|ctx| {
            if let Err(message) = crate::api::ui::bindings::flush_reactive(&reactive, ctx, limits) {
                reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("theme transition: {message}"));
            }
        });
        moved
    }

    /// Claims, one frame: the evaluator aims every property whose winner
    /// changed and moves the presences, before the scene ticks. Free when
    /// no claim has anything to do.
    fn evaluate_claims(&mut self, delta: Duration) -> morf_runtime::animation::claims::ClaimsFrame {
        if !self.reactive.borrow().animation().claims.wants_tick() {
            return Default::default();
        }
        let limits = self.limits;
        let reactive = std::rc::Rc::clone(&self.reactive);
        self.lua.enter(|ctx| {
            crate::runtime::state::run_claims(
                &reactive,
                ctx,
                limits,
                crate::runtime::state::ClaimsPass::Tick(delta),
            )
        })
    }

    /// [`Self::tick_animations`] for a loop driven by a display's frames:
    /// an animation asked for since the last frame starts on this one, its
    /// first frame drawn at its start however long the gap before it was
    /// (see `Scene::set_start_on_tick`).
    pub fn tick_frame_animations(&mut self, delta: Duration) -> Result<AnimationFrame, Error> {
        self.reactive
            .borrow_mut()
            .scene_mut()
            .set_start_on_tick(true);
        self.tick_animations(delta)
    }

    pub fn tick_animations(&mut self, delta: Duration) -> Result<AnimationFrame, Error> {
        let _stage = crate::profile::stage(crate::profile::Stage::Motion);
        self.reactive.borrow_mut().animation_mut().ticked = true;
        let claims = self.evaluate_claims(delta);
        let fading = self.advance_theme_fades(delta);
        {
            let mut state = self.reactive.borrow_mut();
            if state
                .scene_mut()
                .has_running_animation(morf_scene::Element::Timer, "interval")
            {
                // Check before the tick removes a completed animation: the
                // final interval also has to reach the native timer.
                state.revisions_mut().unsettle_services();
            }
        }
        let mut frame = self
            .reactive
            .borrow_mut()
            .scene_mut()
            .tick_animations(delta)
            .map_err(|error| Error::Runtime(error.to_string()))?;
        // A theme fade is motion the scene does not know of: the loop reads
        // this frame to decide whether to keep the frames coming, and a fade
        // it did not see stood still until something else drew.
        if fading > 0 {
            frame.active = true;
            frame.changed += fading;
        }
        // So is a presence: what binds it moves with it.
        frame.active |= claims.moving;
        frame.changed += claims.presences.len();
        // Whatever follows a node that just moved moves with it, this tick.
        frame.changed += crate::runtime::state::apply_follows(&mut self.reactive.borrow_mut());
        // Nodes whose exit has ended go now, with their `on_destroyed` hooks.
        for &node in &frame.exited {
            self.lua.enter(|ctx| {
                crate::runtime::helpers::finish_node_exit(&self.reactive, ctx, self.limits, node);
            });
        }
        if frame.events.is_empty() && frame.groups.is_empty() {
            return Ok(frame);
        }
        // A group callback is registered once and fires once, so it is taken
        // out as it is collected rather than left to leak.
        let finished = self.reactive.borrow_mut().animation_mut().finished(&frame);
        for warning in morf_runtime::animation::report_finished(self, finished) {
            self.reactive.borrow_mut().log(LogLevel::Warn, warning);
        }
        Ok(frame)
    }
}
