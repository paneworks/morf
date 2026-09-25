use std::time::Duration;

use morf_scene::AnimationFrame;

use crate::{api_animation::*, reactive_execute::*, surface_types::*, types::*};

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

    pub fn has_motion(&self) -> bool {
        let state = self.reactive.borrow();
        state.scene.has_motion() || !state.theme_fades.is_empty()
    }

    /// Moves every theme colour easing to a new value on by `delta`, and
    /// hands each reader the colour on show.
    fn advance_theme_fades(&mut self, delta: Duration) {
        let writes = {
            let mut state = self.reactive.borrow_mut();
            if state.theme_fades.is_empty() {
                return;
            }
            let mut writes = Vec::new();
            state.theme_fades.retain_mut(|fade| {
                fade.elapsed += delta;
                let (colour, done) = fade.colour();
                writes.push((fade.signal, IpcValue::Color(colour)));
                !done
            });
            writes
        };
        {
            let mut state = self.reactive.borrow_mut();
            for (id, value) in writes {
                if let Some(graph) = state.graph.as_mut()
                    && graph.write(id, value.clone()).is_ok()
                {
                    state.values.insert(id, value);
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
        self.advance_theme_fades(delta);
        let frame = self
            .reactive
            .borrow_mut()
            .scene
            .tick_animations(delta)
            .map_err(|error| Error::Runtime(error.to_string()))?;
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
        // out of the map as it is collected rather than left to leak.
        let finished = {
            let mut state = self.reactive.borrow_mut();
            let mut finished = frame
                .events
                .iter()
                .filter_map(|event| {
                    let key = (event.node, event.property.to_owned());
                    let callback = state.animation_callbacks.get(&key)?.clone();
                    Some((callback, event.property.to_owned(), event.end, "behavior"))
                })
                .collect::<Vec<_>>();
            for event in &frame.groups {
                if let Some(callback) = state.group_callbacks.remove(&event.group) {
                    finished.push((callback, String::new(), event.end, "animation group"));
                }
            }
            finished
        };
        for (callback, property, end, source) in finished {
            let mut args = vec![IpcValue::String(animation_end_name(end).to_owned())];
            if !property.is_empty() {
                args.insert(0, IpcValue::String(property));
            }
            if let Err(message) =
                self.run_handler(|ctx, limits| execute_handler_args(ctx, &callback, &args, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("{source} on_finished: {message}"));
            }
        }
        Ok(frame)
    }
}
