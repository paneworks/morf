//! The scene's clock: motion scale, what is running, and the tick that
//! advances every animation, group and spring.

use animato::Update;
use std::time::Duration;

use crate::{animation::*, motion::*, motion_values::*, types::*};

impl Scene {
    /// Advances every active behavior without invoking Lua.
    /// Sets how fast motion runs against the clock.
    ///
    /// 1 is real time. 0 is what a reduced-motion preference means: every
    /// behavior, group and spring lands on its target on the next tick, so a
    /// configuration written with motion still ends up in the same place,
    /// only without the travel.
    pub fn set_motion_scale(&mut self, scale: f64) {
        self.motion_scale = if scale.is_finite() {
            scale.max(0.0)
        } else {
            1.0
        };
    }

    /// How fast motion runs against the clock; see [`Self::set_motion_scale`].
    pub fn motion_scale(&self) -> f64 {
        self.motion_scale
    }

    /// Whether an animation's clock starts on the first tick after it was
    /// asked for, rather than being charged that tick's whole delta.
    ///
    /// A loop driven by a display wants this. There a tick's delta is the
    /// time since the previous frame, and an animation asked for between
    /// two frames did not exist for most of it -- for all of it, and then
    /// some, when the turn that asked also built a panel and held the loop.
    /// Charged the delta, a 380 ms morph whose first frame came 100 ms late
    /// is drawn a quarter of the way there on its first frame, and the eye
    /// sees it jump. Started on the tick, its first frame is its start, as
    /// it is when the shell was idle before it. Off by default: a clock
    /// driven by hand (a test, a headless run) means its deltas exactly.
    pub fn set_start_on_tick(&mut self, on: bool) {
        self.start_on_tick = on;
    }

    /// Whether a tick would move anything: an animation or group that is
    /// running rather than paused, or an ending not yet reported. What a loop
    /// asks before it keeps a clock ticking for motion.
    pub fn has_motion(&self) -> bool {
        // An animation on a node nothing shows is not motion (see
        // `scene_shown`): it catches up when the loop next turns.
        let mut shown = std::collections::HashMap::new();
        !self.events.is_empty()
            || !self.group_events.is_empty()
            || self.animations.iter().any(|(key, animation)| {
                !animation.is_paused() && self.change_shows(key.node, key.property, &mut shown)
            })
            || self.groups.values().any(|group| !group.paused)
            || self.stretch_moving()
    }

    /// The animations that are running rather than paused: node, property,
    /// and whether it loops forever. For diagnostics -- what keeps an idle
    /// shell drawing.
    pub fn running_animations(&self) -> Vec<(NodeHandle, &'static str, bool)> {
        self.animations
            .iter()
            .filter(|(_, animation)| !animation.is_paused())
            .map(|(key, animation)| (NodeHandle(key.node), key.property, !animation.settles()))
            .collect()
    }

    /// Whether an unpaused animation of this element kind advances a property.
    /// Unlike the diagnostic listing, this needs no per-frame allocation.
    pub fn has_running_animation(&self, element: Element, property: &str) -> bool {
        let matches = |key: &PropertyKey| {
            key.property == property
                && self
                    .nodes
                    .get(key.node)
                    .is_some_and(|node| node.element == element)
        };
        self.animations
            .iter()
            .any(|(key, animation)| !animation.is_paused() && matches(key))
            || self
                .physics
                .keys()
                .any(|key| !self.paused_physics.contains(key) && matches(key))
    }

    pub fn tick_animations(&mut self, delta: Duration) -> Result<AnimationFrame, SceneError> {
        let snap = self.motion_scale == 0.0;
        // A scale of zero is a tick long enough to finish anything timed. A
        // spring is not timed, so it is landed by hand below.
        let delta = if snap {
            Duration::from_secs(1 << 20)
        } else {
            delta.mul_f64(self.motion_scale)
        };
        // The stretch springs step when the frame's positions are seen, after
        // layout; here their clock only moves, so a second surface reporting
        // on the same tick is recognised as the same moment.
        if !snap {
            self.stretch_clock += delta.as_secs_f64();
        }
        let mut frame = AnimationFrame {
            groups: self.tick_groups(delta)?,
            events: std::mem::take(&mut self.events),
            ..AnimationFrame::default()
        };
        let keys: Vec<_> = self.animations.keys().copied().collect();
        let mut finished = Vec::new();
        // What is seen to move: an animation on a node nothing shows still
        // advances here, but it is not motion the loop draws for (see
        // `scene_shown`).
        let mut shown = std::collections::HashMap::new();
        let mut seen_moving = false;
        for key in keys {
            let animation = self
                .animations
                .get_mut(&key)
                .expect("animation key vanished");
            let paused = animation.is_paused();
            let delayed = animation.is_delayed();
            let fresh = std::mem::take(&mut animation.fresh);
            let step = if fresh && self.start_on_tick {
                0.0
            } else {
                delta.as_secs_f32()
            };
            let complete = !animation.clock.update(step);
            // A settling animation lands exactly on its target; an endless one
            // is stopped at whatever point in the cycle the clock reports.
            let value = if complete && animation.settles() {
                animation.settled().clone()
            } else {
                animation.value()
            };
            let Some(node) = self.nodes.get(key.node) else {
                finished.push(key);
                continue;
            };
            // A paused clock holds its value, and one still draining its delay
            // has not left the start value, so neither is worth a repaint.
            let idle = paused || (delayed && animation.is_delayed());
            if !idle {
                let slot = node.properties[key.property];
                // A tick that moved nothing (a zero delta: the frame after one
                // nothing on show moved) changes nothing to lay out or draw.
                // Bumping anyway made every callback a fresh layout, and that
                // paint asked for the next callback: hidden motion kept the
                // surface repainting forever.
                let unchanged = !complete
                    && self
                        .properties
                        .read(slot.current)
                        .is_ok_and(|now| *now == value);
                if !unchanged {
                    if affects_layout(key.property) {
                        self.bump_layout(key.node);
                    }
                    self.properties.write(slot.current, value)?;
                }
                if self.change_shows(key.node, key.property, &mut shown) {
                    frame.changed += usize::from(!unchanged);
                    seen_moving |= !complete;
                }
            } else if !paused && !complete && self.change_shows(key.node, key.property, &mut shown)
            {
                // Waiting out a delay, and on show: it will move.
                seen_moving = true;
            }
            if complete {
                finished.push(key);
            }
        }
        for key in finished {
            if self.animations.remove(&key).is_some() {
                self.settle_target(key)?;
            }
            frame.events.push(AnimationEvent {
                node: NodeHandle(key.node),
                property: key.property,
                end: AnimationEnd::Completed,
            });
        }
        let physics_keys: Vec<_> = self.physics.keys().copied().collect();
        let mut physics_finished = Vec::new();
        for key in physics_keys {
            if self.paused_physics.contains(&key) {
                continue;
            }
            let Some(node) = self.nodes.get(key.node) else {
                physics_finished.push(key);
                continue;
            };
            let slot = node.properties[key.property];
            if snap {
                let target = self.properties.read(slot.target)?.clone();
                if affects_layout(key.property) {
                    self.bump_layout(key.node);
                }
                self.properties.write(slot.current, target)?;
                frame.changed += 1;
                physics_finished.push(key);
                continue;
            }
            let motion = self.physics.get_mut(&key).expect("physics key vanished");
            if let PhysicsAnimation::Color { channels } = motion {
                let Value::Color(mut current) = *self.properties.read(slot.current)? else {
                    physics_finished.push(key);
                    continue;
                };
                let settled = advance_physics_color(channels, &mut current, delta);
                self.properties.write(slot.current, Value::Color(current))?;
                let colour_node = key.node;
                let colour_property = key.property;
                if self.change_shows(colour_node, colour_property, &mut shown) {
                    frame.changed += 1;
                    seen_moving |= !settled;
                }
                if settled {
                    physics_finished.push(key);
                }
                continue;
            }
            let Value::Number(mut current) = *self.properties.read(slot.current)? else {
                physics_finished.push(key);
                continue;
            };
            let before = current;
            let settled = advance_physics(motion, &mut current, delta);
            // Physics moves a property without any assignment, so this is the
            // one write that has to say so itself. Without it a paint reuses
            // the layout it already had and the scene animates behind a still
            // picture — every other path reaches here through `assign`. A
            // spring that did not move (a zero delta, as above) says nothing.
            let moved = current != before;
            if moved {
                if affects_layout(key.property) {
                    self.bump_layout(key.node);
                }
                self.properties
                    .write(slot.current, Value::Number(current))?;
            }
            if self.change_shows(key.node, key.property, &mut shown) {
                frame.changed += usize::from(moved);
                seen_moving |= !settled;
            }
            if settled {
                physics_finished.push(key);
            }
        }
        for key in physics_finished {
            self.physics.remove(&key);
            self.paused_physics.remove(&key);
            self.settle_target(key)?;
            frame.events.push(AnimationEvent {
                node: NodeHandle(key.node),
                property: key.property,
                end: AnimationEnd::Completed,
            });
        }
        frame.exited = self.finished_exits();
        // Only what is seen to move asks for the next frame; groups are
        // timelines that start their steps themselves, and stay motion.
        frame.active =
            seen_moving || self.groups.values().any(|group| !group.paused) || self.stretch_moving();
        Ok(frame)
    }
}
