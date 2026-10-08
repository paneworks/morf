//! Motion under physics: checking a spring or a coast, starting one, and
//! advancing it a tick.

use animato::{Spring, SpringConfig, Update};
use std::time::Duration;

use crate::animation::*;

pub(crate) fn validate_physics(physics: Physics) -> Result<(), String> {
    match physics {
        Physics::Spring {
            mass,
            damping,
            stiffness,
            epsilon,
        } if mass.is_finite()
            && mass > 0.0
            && damping.is_finite()
            && damping >= 0.0
            && stiffness.is_finite()
            && stiffness > 0.0
            && epsilon.is_finite()
            && epsilon > 0.0 =>
        {
            Ok(())
        }
        Physics::Smoothed { velocity } if velocity.is_finite() && velocity > 0.0 => Ok(()),
        Physics::Decay {
            friction,
            min_velocity,
            bounds,
            gravity,
            restitution,
        } if friction.is_finite()
            && friction >= 0.0
            && min_velocity.is_finite()
            && min_velocity >= 0.0
            && gravity.is_finite()
            && restitution.is_finite()
            && (0.0..=1.0).contains(&restitution)
            && bounds
                .is_none_or(|(low, high)| low.is_finite() && high.is_finite() && low <= high) =>
        {
            Ok(())
        }
        Physics::Spring { .. } => Err("spring values must be finite and physically valid".into()),
        Physics::Smoothed { .. } => {
            Err("smoothed velocity must be finite and greater than zero".into())
        }
        Physics::Decay { .. } => Err(
            "decay needs finite friction, gravity and minimum velocity, a restitution between \
             zero and one, and ordered bounds"
                .into(),
        ),
    }
}

pub(crate) fn physics_animation(
    current: f64,
    target: f64,
    velocity: f64,
    spec: Physics,
) -> PhysicsAnimation {
    match spec {
        // Decay pursues nothing, so there is no target for an assignment to
        // set. It is installed by `Scene::fling`, which is why `set_physics`
        // refuses it and this arm cannot be reached.
        Physics::Decay { .. } => unreachable!("decay is started by a fling, not by an assignment"),
        Physics::Spring {
            mass,
            damping,
            stiffness,
            epsilon,
        } => PhysicsAnimation::Spring {
            target,
            motion: Spring::from_velocity(
                current as f32,
                velocity as f32,
                target as f32,
                SpringConfig {
                    stiffness: stiffness as f32,
                    damping: damping as f32,
                    mass: mass as f32,
                    epsilon: epsilon as f32,
                },
            ),
        },
        Physics::Smoothed { velocity: limit } => PhysicsAnimation::Smoothed {
            target,
            velocity,
            limit,
        },
    }
}

/// Whether a coast has run out of speed, given the slowest it may travel.
///
/// Strictly slower, so that a `min_velocity` of zero means what it says: no
/// speed is too slow, and the coast ends only when something else ends it.
/// That is the one way a configuration can say "this property is under physics
/// and currently still" — which is what anything driving a property by force
/// rather than by throw needs, because a coast that has ended takes no
/// impulses, and a property stopped dead this way could never be pushed again.
pub(crate) fn slow_enough_to_stop(velocity: f64, min_velocity: f64) -> bool {
    min_velocity > 0.0 && velocity.abs() <= min_velocity
}

pub(crate) fn advance_physics(
    motion: &mut PhysicsAnimation,
    current: &mut f64,
    delta: Duration,
) -> bool {
    let seconds = delta.as_secs_f64();
    match motion {
        PhysicsAnimation::Scroll(scroll) => {
            let finished = scroll.advance(delta);
            *current = scroll.position;
            finished
        }
        PhysicsAnimation::Spring {
            target,
            motion: spring,
        } => {
            let steps = (seconds / (1.0 / 120.0)).ceil().max(1.0) as usize;
            let step = (seconds / steps as f64) as f32;
            let mut active = true;
            for _ in 0..steps {
                active = spring.update(step);
            }
            *current = f64::from(spring.position());
            if !active {
                *current = *target;
                true
            } else {
                false
            }
        }
        PhysicsAnimation::Decay {
            position,
            velocity,
            friction,
            gravity,
            restitution,
            min_velocity,
            bounds,
        } => {
            // Semi-implicit Euler: accelerate, then move. Friction opposes the
            // motion and may bring it to a stop within the step, so it is
            // clamped rather than allowed to push the other way.
            *velocity += *gravity * seconds;
            let drag = *friction * seconds;
            if drag >= velocity.abs() {
                *velocity = 0.0;
            } else {
                *velocity -= drag * velocity.signum();
            }
            *position += *velocity * seconds;

            let mut resting = false;
            if let Some((low, high)) = *bounds {
                // A bound returns the speed it did not absorb, which is what
                // makes it a bounce rather than a wall.
                if *position < low {
                    *position = low;
                    *velocity = -*velocity * *restitution;
                } else if *position > high {
                    *position = high;
                    *velocity = -*velocity * *restitution;
                }
                // At rest means: too slow to leave, against the bound that
                // gravity holds it to. Without gravity either end will do.
                let held = (*position - low).abs() < f64::EPSILON && *gravity < 0.0
                    || (*position - high).abs() < f64::EPSILON && *gravity > 0.0
                    || *gravity == 0.0
                        && ((*position - low).abs() < f64::EPSILON
                            || (*position - high).abs() < f64::EPSILON);
                resting = held && slow_enough_to_stop(*velocity, *min_velocity);
            }
            *current = *position;
            if resting {
                *velocity = 0.0;
                return true;
            }
            // Running out of speed ends a coast, but not a fall: with gravity
            // the next step will start it moving again.
            *gravity == 0.0 && slow_enough_to_stop(*velocity, *min_velocity)
        }
        PhysicsAnimation::Color { .. } => true,
        PhysicsAnimation::Smoothed {
            target,
            velocity,
            limit,
        } => {
            let distance = *target - *current;
            let step = *limit * seconds;
            if distance.abs() <= step {
                *current = *target;
                *velocity = 0.0;
                true
            } else {
                *velocity = limit.copysign(distance);
                *current += *velocity * seconds;
                false
            }
        }
    }
}
