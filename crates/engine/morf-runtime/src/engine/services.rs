//! The engine's half of a turn's services: native timers kept in step with
//! the scene's `Timer` nodes.

use std::time::Duration;

use crate::timers::Timer;

use super::Engine;

impl Engine {
    /// Brings the native timers in line with the `Timer` nodes, when the scene
    /// changed. Returns whether anything did.
    pub fn reconcile_timers(&mut self, definitions_changed: bool) -> bool {
        let mut service_changed = false;
        let timer_definitions = if definitions_changed {
            self.timer_callbacks
                .iter()
                .map(|(node, callback)| (*node, callback.clone()))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut stale_timers = Vec::new();
        for (node, callback) in timer_definitions {
            let Ok(running) = self.scene.bool_value(node, "running") else {
                stale_timers.push(node);
                continue;
            };
            let interval = self.scene.number(node, "interval").unwrap_or(0.0);
            let repeat = self.scene.bool_value(node, "repeat").unwrap_or(false);
            let duration = (interval.is_finite() && interval > 0.0)
                .then(|| Duration::from_secs_f64(interval / 1_000.0));
            if !running || duration.is_none() {
                service_changed |= self.timers.remove_node(node);
                continue;
            }
            let duration = duration.expect("validated duration");
            let matches = self
                .timers
                .for_node(node)
                .is_some_and(|timer| timer.interval == duration && timer.repeat == repeat);
            if matches {
                continue;
            }
            self.timers.remove_node(node);
            match self.timers.source(duration) {
                Ok(source) => {
                    let id = self.timers.next_id();
                    let origin = self
                        .timer_origins
                        .get(&node)
                        .cloned()
                        .unwrap_or_else(|| format!("ui.Timer {node:?}").into());
                    self.timers.add(Timer {
                        id,
                        source,
                        handler: callback,
                        repeat,
                        interval: duration,
                        node: Some(node),
                        origin,
                    });
                }
                Err(error) => self.log(crate::log::LogLevel::Warn, format!("Timer: {error}")),
            }
            service_changed = true;
        }
        for node in stale_timers {
            self.timer_callbacks.remove(&node);
            self.timers.remove_node(node);
            self.timer_origins.remove(&node);
        }
        service_changed
    }
}
