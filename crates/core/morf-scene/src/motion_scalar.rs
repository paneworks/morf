//! A number beside the tree moved by the scene's own motion: a tween's
//! clock and easing, or a spring, retargeted with its speed carried, and
//! advanced on the scene's clock by [`Scene::advance_scalar`].

use animato::Update;
use std::time::Duration;

use crate::animation::*;
use crate::motion::{advance_physics, physics_animation, validate_physics};
use crate::types::{Scene, Value};

#[derive(Clone, Debug)]
enum Running {
    Tween(Box<Animation>),
    Physics(PhysicsAnimation),
}

/// One number that moves the way a property does.
#[derive(Clone, Debug)]
pub struct ScalarMotion {
    value: f64,
    target: f64,
    running: Option<Running>,
}

impl ScalarMotion {
    /// At rest on `value`.
    pub fn new(value: f64) -> Self {
        Self {
            value,
            target: value,
            running: None,
        }
    }

    /// Where it is now.
    pub fn value(&self) -> f64 {
        self.value
    }

    /// Where it is going.
    pub fn target(&self) -> f64 {
        self.target
    }

    /// Whether a motion is still under way.
    pub fn is_moving(&self) -> bool {
        self.running.is_some()
    }

    /// Its speed now, in units a second.
    pub fn velocity(&self) -> f64 {
        match &self.running {
            Some(Running::Tween(animation)) if !animation.is_paused() => {
                match animation.velocity() {
                    Velocity::Number(velocity) => velocity,
                    Velocity::Color(_) => 0.0,
                }
            }
            Some(Running::Physics(physics)) => physics.velocity(),
            _ => 0.0,
        }
    }

    /// Aims at `target` from where it is, carrying its speed into `motion`.
    /// With no motion, or one that takes no time, it is there at once.
    pub fn retarget(&mut self, target: f64, motion: Option<Motion>) {
        if target == self.target {
            return;
        }
        let velocity = self.velocity();
        self.target = target;
        self.running = match motion {
            Some(Motion::Physics(spec))
                if !matches!(spec, Physics::Decay { .. }) && validate_physics(spec).is_ok() =>
            {
                Some(Running::Physics(physics_animation(
                    self.value, target, velocity, spec,
                )))
            }
            Some(Motion::Tween(behavior)) if behavior.intercepts() => {
                let carried = if behavior.keep_velocity {
                    velocity
                } else {
                    0.0
                };
                let initial = Velocity::Number(carried);
                Some(Running::Tween(Box::new(Animation::new(
                    Value::Number(self.value),
                    Value::Number(target),
                    initial,
                    initial.is_moving(),
                    behavior,
                ))))
            }
            _ => None,
        };
        if self.running.is_none() {
            self.value = target;
        }
    }
}

impl Scene {
    /// Moves `motion` on by `delta` the way this tick moves a property: at
    /// the scene's motion scale, landing at once when motion is off, and a
    /// fresh tween starting on the tick when the clock asks for that.
    /// Returns whether it still moves.
    pub fn advance_scalar(&self, motion: &mut ScalarMotion, delta: Duration) -> bool {
        let snap = self.motion_scale == 0.0;
        let delta = if snap {
            Duration::from_secs(1 << 20)
        } else {
            delta.mul_f64(self.motion_scale)
        };
        let done = match &mut motion.running {
            None => return false,
            Some(Running::Tween(animation)) => {
                let fresh = std::mem::take(&mut animation.fresh);
                let step = if fresh && self.start_on_tick {
                    0.0
                } else {
                    delta.as_secs_f32()
                };
                let complete = !animation.clock.update(step);
                let value = if complete {
                    animation.settled().clone()
                } else {
                    animation.value()
                };
                if let Value::Number(value) = value {
                    motion.value = value;
                }
                complete
            }
            Some(Running::Physics(_)) if snap => {
                motion.value = motion.target;
                true
            }
            Some(Running::Physics(physics)) => advance_physics(physics, &mut motion.value, delta),
        };
        if done {
            motion.running = None;
            motion.value = motion.target;
        }
        !done
    }
}
