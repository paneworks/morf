//! What a loop asks of the engine between turns, answered from its state
//! alone: whether anything moves, what comes due next, whether a turn is
//! owed, where keys and the wheel go, and the reports the logs print.

use std::time::{Duration, Instant};

use morf_scene::{NodeHandle, Scene};
use morf_value::IpcValue;

use crate::events::{UiEvent, routing};
use crate::wake::{ClockPrecision, DeadlineCause};

use super::Engine;

/// How long a preload waits for the scene to stand still before it is made
/// anyway.
pub const PRELOAD_PATIENCE: Duration = Duration::from_millis(1500);

/// A node's place in its tree, by element, at most four deep: how the lint
/// and the reports name a node with no id.
pub fn node_path(scene: &Scene, node: NodeHandle) -> String {
    let mut names = Vec::new();
    let mut current = Some(node);
    while let Some(at) = current {
        if names.len() == 4 {
            names.push("…".to_owned());
            break;
        }
        names.push(
            scene
                .element(at)
                .map(|element| format!("{element:?}"))
                .unwrap_or_default(),
        );
        current = scene.parent(at).ok().flatten();
    }
    names.reverse();
    names.join(" > ")
}

/// ` #id` for a node with one, nothing otherwise.
fn id_suffix(scene: &Scene, node: NodeHandle) -> String {
    scene
        .string_value(node, "id")
        .ok()
        .filter(|id| !id.is_empty())
        .map(|id| format!(" #{id}"))
        .unwrap_or_default()
}

impl Engine {
    /// Whether anything is in motion: the scene's animations or a theme fade.
    pub fn has_motion(&self) -> bool {
        self.scene.has_motion() || self.animation.fading()
    }

    /// The finest clock anything currently reads, or nothing when no binding
    /// shows the time.
    pub fn clock_precision(&self) -> Option<ClockPrecision> {
        self.clocks.precision(&self.reactive)
    }

    /// When the press being held becomes a long press, if one is held where
    /// a long press is wanted.
    pub fn long_press_due(&self) -> Option<Instant> {
        let due = self
            .gestures
            .long_press_due(|node| self.events.has(node, UiEvent::LongPressed))?;
        Some(crate::gestures::wall(self.timers.virtual_now(), due))
    }

    /// The earliest moment the engine's own work comes due on the wall
    /// clock -- a timer, a preload, a long press -- and what it is.
    pub fn next_deadline(&self) -> Option<(Instant, DeadlineCause)> {
        let timers = self
            .timers
            .next_wall_deadline()
            .map(|at| (at, DeadlineCause::Timer));
        // Due at once while the scene is still; moving, only once it has
        // waited as long as a preload waits for anything.
        let still = !self.scene.has_motion();
        let preload = self
            .retained
            .preload_pending
            .values()
            .min()
            .map(|since| {
                if still {
                    *since
                } else {
                    *since + PRELOAD_PATIENCE
                }
            })
            .map(|at| (at, DeadlineCause::Preload));
        let long_press = self
            .long_press_due()
            .map(|at| (at, DeadlineCause::LongPress));
        crate::wake::earliest([timers, preload, long_press])
    }

    /// Whether the last turn left the engine work no thread will ring for:
    /// the scene changed since the services last ran, a node waits to be torn
    /// down, a model a view follows changed.
    pub fn has_pending_work(&self) -> bool {
        self.revisions.scene_revision != self.revisions.polled_revision
            || self.revisions.scene_revision != self.revisions.service_definitions_revision
            || !self.retained.retained_destroy_queue.is_empty()
            || self
                .views
                .values()
                .any(|view| view.model.borrow().has_changes())
    }

    /// Moves every theme colour easing on by `delta` and writes the colour
    /// on show to each one's signal. Returns how many moved; the caller
    /// flushes what read them.
    pub fn advance_theme_fades(&mut self, delta: Duration) -> usize {
        if !self.animation.fading() {
            return 0;
        }
        let writes = crate::animation::fades::advance(&mut self.animation.fades, delta);
        let moved = writes.len();
        for (signal, colour) in writes {
            let value = IpcValue::Color(colour);
            if let Some(graph) = self.reactive.graph.as_mut()
                && graph.write(signal, value.clone()).is_ok()
            {
                self.reactive.values.insert(signal, value);
            }
        }
        moved
    }

    /// Tells every stretching node in a frame's layout where it is, so its
    /// spring steps before the frame is painted. Free when nothing stretches.
    pub fn observe_stretch(&mut self, layout: &morf_layout::Layout) {
        if !self.scene.has_stretch() {
            return;
        }
        if let Err(error) = morf_layout::observe_stretch(&mut self.scene, layout) {
            self.log(crate::log::LogLevel::Warn, format!("stretch: {error}"));
        }
    }

    /// The node a key pressed while `node` has focus goes to.
    pub fn key_route(&self, node: NodeHandle) -> Option<NodeHandle> {
        routing::key_route(&self.scene, &self.events, node)
    }

    /// Whether `node` takes the wheel rather than letting it bubble on.
    pub fn takes_wheel(&self, node: NodeHandle) -> bool {
        routing::takes_wheel(&self.scene, &self.events, node)
    }

    /// Whether a pointer area accepts one Linux input button code.
    pub fn accepts_pointer_button(&self, node: NodeHandle, button: u32) -> bool {
        routing::accepts_pointer_button(&self.scene, node, button)
    }

    /// What is animating right now, one line each (`path.property`, marked
    /// when it loops forever), at most `max`.
    pub fn motion_report(&self, max: usize) -> Vec<String> {
        let scene = &self.scene;
        let mut lines: Vec<String> = scene
            .running_animations()
            .into_iter()
            .map(|(node, property, endless)| {
                format!(
                    "{}{}.{property}{}",
                    node_path(scene, node),
                    id_suffix(scene, node),
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
    /// how many there were.
    pub fn layout_report(&self, root: NodeHandle, since: u64, max: usize) -> (usize, Vec<String>) {
        let scene = &self.scene;
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
                    lines.push(format!(
                        "{}{}",
                        node_path(scene, node),
                        id_suffix(scene, node)
                    ));
                }
            }
            if let Ok(children) = scene.children(node) {
                pending.extend(children.iter().copied());
            }
        }
        (count, lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use morf_scene::Element;

    #[test]
    fn a_still_engine_owes_nothing_and_names_nodes_by_their_path() {
        let mut engine = Engine::new();
        assert!(!engine.has_motion());
        assert!(engine.next_deadline().is_none());
        assert_eq!(engine.advance_theme_fades(Duration::from_millis(16)), 0);
        let root = engine.scene.create(Element::Item);
        let child = engine.scene.create(Element::Rect);
        engine.scene.reparent(child, Some(root)).unwrap();
        assert_eq!(node_path(&engine.scene, child), "Item > Rect");
        assert!(engine.motion_report(8).is_empty());
    }

    #[test]
    fn a_preload_is_due_at_once_while_the_scene_is_still() {
        let mut engine = Engine::new();
        let node = engine.scene.create(Element::Item);
        let since = Instant::now();
        engine.retained.preload_pending.insert(node, since);
        assert_eq!(
            engine.next_deadline(),
            Some((since, DeadlineCause::Preload))
        );
    }
}
