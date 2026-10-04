use morf_scene::NodeHandle;
use std::time::Duration;

use morf_scene::AnimationFrame;

use crate::{surface_types::*, types::*};

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
    /// when it loops forever), at most `max`: for `MORF_WAKE_LOG`, which asks
    /// what keeps an otherwise idle shell drawing frames.
    pub fn motion_report(&self, max: usize) -> Vec<String> {
        let state = self.reactive.borrow();
        let scene = &state.scene;
        let mut lines: Vec<String> = scene
            .running_animations()
            .into_iter()
            .map(|(node, property, endless)| {
                let id = scene
                    .string_value(node, "id")
                    .ok()
                    .filter(|id| !id.is_empty())
                    .map(|id| format!(" #{id}"))
                    .unwrap_or_default();
                format!(
                    "{}{id}.{property}{}",
                    crate::runtime_config::lint_path(scene, node),
                    if endless { " (loops)" } else { "" }
                )
            })
            .collect();
        lines.sort();
        lines.dedup();
        lines.truncate(max);
        lines
    }

    /// What changed for layout under `root` since the layout at revision
    /// `since`: the nodes stamped since, one line each, at most `max`, and
    /// how many there were. For `MORF_FRAME_LOG=2`, when a layout is slow.
    pub fn layout_report(&self, root: NodeHandle, since: u64, max: usize) -> (usize, Vec<String>) {
        let state = self.reactive.borrow();
        let scene = &state.scene;
        let mut count = 0;
        let mut lines = Vec::new();
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            let Ok(stamps) = scene.layout_stamps(node) else {
                continue;
            };
            if stamps.subtree <= since {
                continue;
            }
            if stamps.own > since {
                count += 1;
                if lines.len() < max {
                    let id = scene
                        .string_value(node, "id")
                        .ok()
                        .filter(|id| !id.is_empty())
                        .map(|id| format!(" #{id}"))
                        .unwrap_or_default();
                    lines.push(format!(
                        "{}{id}",
                        crate::runtime_config::lint_path(scene, node)
                    ));
                }
            }
            if let Ok(children) = scene.children(node) {
                pending.extend(children.iter().copied());
            }
        }
        (count, lines)
    }

    pub fn has_motion(&self) -> bool {
        let state = self.reactive.borrow();
        state.scene.has_motion() || state.animation.fading()
    }

    /// Moves every theme colour easing to a new value on by `delta`, and
    /// hands each reader the colour on show.
    /// Returns how many colours moved.
    fn advance_theme_fades(&mut self, delta: Duration) -> usize {
        let writes = {
            let mut state = self.reactive.borrow_mut();
            if !state.animation.fading() {
                return 0;
            }
            morf_runtime::animation::fades::advance(&mut state.animation.fades, delta)
                .into_iter()
                .map(|(signal, colour)| (signal, IpcValue::Color(colour)))
                .collect::<Vec<_>>()
        };
        let moved = writes.len();
        {
            let mut state = self.reactive.borrow_mut();
            for (id, value) in writes {
                if let Some(graph) = state.reactive.graph.as_mut()
                    && graph.write(id, value.clone()).is_ok()
                {
                    state.reactive.values.insert(id, value);
                }
            }
        }
        let limits = self.limits;
        let reactive = std::rc::Rc::clone(&self.reactive);
        self.lua.enter(|ctx| {
            if let Err(message) = crate::reactive_bindings::flush_reactive(&reactive, ctx, limits) {
                reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("theme transition: {message}"));
            }
        });
        moved
    }

    /// [`Self::tick_animations`] for a loop driven by a display's frames:
    /// an animation asked for since the last frame starts on this one, its
    /// first frame drawn at its start however long the gap before it was
    /// (see `Scene::set_start_on_tick`).
    pub fn tick_frame_animations(&mut self, delta: Duration) -> Result<AnimationFrame, Error> {
        self.reactive.borrow_mut().scene.set_start_on_tick(true);
        self.tick_animations(delta)
    }

    pub fn tick_animations(&mut self, delta: Duration) -> Result<AnimationFrame, Error> {
        let fading = self.advance_theme_fades(delta);
        {
            let mut state = self.reactive.borrow_mut();
            if state
                .scene
                .has_running_animation(morf_scene::Element::Timer, "interval")
            {
                // Check before the tick removes a completed animation: the
                // final interval also has to reach the native timer.
                state.revisions.service_definitions_revision =
                    state.revisions.scene_revision.wrapping_sub(1);
            }
        }
        let mut frame = self
            .reactive
            .borrow_mut()
            .scene
            .tick_animations(delta)
            .map_err(|error| Error::Runtime(error.to_string()))?;
        // A theme fade is motion the scene does not know of: the loop reads
        // this frame to decide whether to keep the frames coming, and a fade
        // it did not see stood still until something else drew.
        if fading > 0 {
            frame.active = true;
            frame.changed += fading;
        }
        // Whatever follows a node that just moved moves with it, this tick.
        frame.changed += crate::state::apply_follows(&mut self.reactive.borrow_mut());
        // Nodes whose exit has ended go now, with their `on_destroyed` hooks.
        for &node in &frame.exited {
            self.lua.enter(|ctx| {
                crate::runtime_helpers::finish_node_exit(&self.reactive, ctx, self.limits, node);
            });
        }
        if frame.events.is_empty() && frame.groups.is_empty() {
            return Ok(frame);
        }
        // A group callback is registered once and fires once, so it is taken
        // out as it is collected rather than left to leak.
        let finished = self.reactive.borrow_mut().animation.finished(&frame);
        for warning in morf_runtime::animation::report_finished(self, finished) {
            self.reactive.borrow_mut().log(LogLevel::Warn, warning);
        }
        Ok(frame)
    }
}
