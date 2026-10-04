//! Squash and stretch: a node that deforms with its own motion.
//!
//! A node with a [`Stretch`] is watched as it moves. Its velocity decides a
//! deformation it is pulled towards — longer along the direction it travels,
//! narrower across it, the area kept — and a damped spring does the pulling,
//! so the shape lags the motion, overshoots when the motion stops and settles
//! back to square. The deformation is a symmetric 2×2 matrix about the node's
//! centre, applied to its rendered transform: its children deform with it, and
//! a field layer tracking it deforms the same way.
//!
//! The engine measures the velocity itself, from where the node was actually
//! drawn on consecutive frames, so it does not matter what moved it — a
//! behavior on `translate_x`, a spring on `y`, a parent sliding, a layout
//! change. Nothing is asked of the configuration per frame.
//!
//! A node at rest costs nothing: a resting spring asks for no frames, and a
//! frame that happens anyway reads one position and finds it where it was.

use crate::{SceneError, types::*};

/// How a node stretches with its motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stretch {
    /// Spring stiffness, per second squared. Higher follows the motion more
    /// tightly and wobbles faster.
    pub stiffness: f64,
    /// Spring damping, per second. Critical damping is `2 * sqrt(stiffness)`;
    /// below it the shape overshoots and wobbles when the motion stops.
    pub damping: f64,
    /// How much longer the node gets per 1000 logical pixels a second: 0.1 is
    /// ten percent longer (and correspondingly narrower) at that speed.
    pub scale: f64,
    /// The most it may stretch, as a fraction of its length.
    pub max: f64,
}

impl Default for Stretch {
    fn default() -> Self {
        Self {
            stiffness: 260.0,
            damping: 16.0,
            scale: 0.12,
            max: 0.35,
        }
    }
}

impl Stretch {
    /// Reads `true` (the defaults), or a map of any of `stiffness`,
    /// `damping`, `scale` and `max`. `nil` and `false` are no stretch.
    pub fn from_value(value: &Value) -> Result<Option<Self>, String> {
        let fields = match value {
            Value::Nil | Value::Bool(false) => return Ok(None),
            Value::Bool(true) => return Ok(Some(Self::default())),
            Value::Map(fields) => fields,
            _ => {
                return Err(
                    "stretch must be true, false or { stiffness, damping, scale, max }".into(),
                );
            }
        };
        let mut stretch = Self::default();
        for (key, value) in fields {
            let Value::Number(number) = value else {
                return Err(format!("stretch {key} must be a number"));
            };
            if !number.is_finite() {
                return Err(format!("stretch {key} must be finite"));
            }
            match key.as_str() {
                "stiffness" if *number > 0.0 => stretch.stiffness = *number,
                "damping" if *number >= 0.0 => stretch.damping = *number,
                "scale" => stretch.scale = *number,
                "max" if *number >= 0.0 => stretch.max = number.min(4.0),
                "stiffness" => return Err("stretch stiffness must be greater than zero".into()),
                "damping" | "max" => return Err(format!("stretch {key} cannot be negative")),
                other => {
                    return Err(format!(
                        "`{other}` is not a stretch setting: use stiffness, damping, scale, max"
                    ));
                }
            }
        }
        Ok(Some(stretch))
    }

    /// The deformation this velocity pulls towards, as `[xx, xy, yy]` added
    /// to the identity.
    ///
    /// Stretched by `1 + e` along the direction of travel and squashed by
    /// `1 / (1 + e)` across it, so the area stays what it was.
    pub fn target(&self, velocity: [f64; 2]) -> [f64; 3] {
        let speed = velocity[0].hypot(velocity[1]);
        // Under a pixel a second is not motion: a node at rest drawn through
        // a transform lands a hair off where it was, frame to frame, and that
        // noise must not keep it (and the frames) going for ever.
        if speed < STILL_SPEED || self.scale == 0.0 {
            return [0.0; 3];
        }
        let amount = (self.scale * speed / 1000.0).clamp(-self.max.min(0.9), self.max);
        let (x, y) = (velocity[0] / speed, velocity[1] / speed);
        let across = 1.0 / (1.0 + amount) - 1.0;
        [
            amount * x * x + across * y * y,
            (amount - across) * x * y,
            amount * y * y + across * x * x,
        ]
    }
}

/// Slower than this, in surface pixels a second, a node is standing still.
const STILL_SPEED: f64 = 1.0;
/// A deformation this small is none: the shape is at rest.
const REST_VALUE: f64 = 1e-4;

/// The spring's state for one stretching node.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct StretchState {
    pub(crate) spec: Stretch,
    /// Where the node's centre was last seen, and the stretch clock then.
    pub(crate) sample: Option<([f64; 2], f64)>,
    /// The deformation `[xx, xy, yy]` beyond the identity.
    pub(crate) value: [f64; 3],
    /// How fast each component of it is changing, per second.
    pub(crate) rate: [f64; 3],
    /// Whether the spring is still moving; a resting one wants no frames.
    pub(crate) moving: bool,
}

/// Frames further apart than this are not one motion: a node that was
/// somewhere else a second ago jumped, and jumping is not speed.
pub const STRETCH_MAX_GAP: f64 = 0.1;

/// One step of a damped spring pulled towards `target`, solved exactly.
///
/// `x'' = -k (x - target) - c x'` with unit mass. With the target held for
/// the step this has a closed form, so the step is exact whatever its length:
/// a dropped frame does not make the spring gain energy, which an explicit
/// integrator at 60 Hz and this stiffness would. Returns the new position and
/// rate.
pub fn spring_step(
    position: f64,
    rate: f64,
    target: f64,
    stiffness: f64,
    damping: f64,
    seconds: f64,
) -> (f64, f64) {
    let offset = position - target;
    let omega = stiffness.max(1e-9).sqrt();
    let zeta = damping / (2.0 * omega);
    let t = seconds.max(0.0);
    let (offset, rate) = if (zeta - 1.0).abs() < 1e-6 {
        // Critically damped: the one repeated root.
        let decay = (-omega * t).exp();
        let coefficient = rate + omega * offset;
        (
            (offset + coefficient * t) * decay,
            (rate - omega * coefficient * t) * decay,
        )
    } else if zeta < 1.0 {
        let alpha = zeta * omega;
        let damped = omega * (1.0 - zeta * zeta).sqrt();
        let decay = (-alpha * t).exp();
        let (sin, cos) = (damped * t).sin_cos();
        (
            decay * (offset * cos + (rate + alpha * offset) / damped * sin),
            decay * (rate * cos - (alpha * rate + omega * omega * offset) / damped * sin),
        )
    } else {
        let root = (zeta * zeta - 1.0).sqrt();
        let fast = -omega * (zeta + root);
        let slow = -omega * (zeta - root);
        let a = (rate - fast * offset) / (slow - fast);
        let b = offset - a;
        let (ea, eb) = ((slow * t).exp(), (fast * t).exp());
        (a * ea + b * eb, slow * a * ea + fast * b * eb)
    };
    (target + offset, rate)
}

impl StretchState {
    pub(crate) fn new(spec: Stretch) -> Self {
        Self {
            spec,
            ..Self::default()
        }
    }

    /// Takes a sighting of the node's centre at the stretch clock `now`,
    /// with `to_local` turning a surface-space velocity into the frame the
    /// deformation is applied in. Returns whether the deformation changed.
    pub(crate) fn observe(&mut self, centre: [f64; 2], now: f64, to_local: [f64; 4]) -> bool {
        let previous = self.sample.replace((centre, now));
        let Some((before, then)) = previous else {
            return false;
        };
        let seconds = now - then;
        if seconds <= 0.0 {
            // A second sighting on the same tick — another surface's paint.
            return false;
        }
        let velocity = if seconds <= STRETCH_MAX_GAP {
            let surface = [
                (centre[0] - before[0]) / seconds,
                (centre[1] - before[1]) / seconds,
            ];
            [
                to_local[0] * surface[0] + to_local[2] * surface[1],
                to_local[1] * surface[0] + to_local[3] * surface[1],
            ]
        } else {
            [0.0; 2]
        };
        if seconds > STRETCH_MAX_GAP && !self.moving {
            return false;
        }
        let target = self.spec.target(velocity);
        let old = self.value;
        let step = seconds.min(STRETCH_MAX_GAP);
        for ((value, rate), target) in self.value.iter_mut().zip(&mut self.rate).zip(target) {
            let (next, next_rate) = spring_step(
                *value,
                *rate,
                target,
                self.spec.stiffness,
                self.spec.damping,
                step,
            );
            // Kept in a range that is still a shape: nothing flips inside out
            // however hard it is thrown.
            *value = next.clamp(-0.9, 4.0);
            *rate = next_rate;
        }
        let resting = target.iter().all(|value| value.abs() < REST_VALUE)
            && self.value.iter().all(|value| value.abs() < REST_VALUE)
            && self.rate.iter().all(|rate| rate.abs() < 1e-3);
        if resting {
            self.value = [0.0; 3];
            self.rate = [0.0; 3];
        }
        self.moving = !resting;
        self.value != old
    }

    /// The deformation as a linear map, `[a, b, c, d]` column major, or
    /// nothing at the identity.
    pub(crate) fn matrix(&self) -> Option<[f64; 4]> {
        (self.value != [0.0; 3]).then(|| {
            [
                1.0 + self.value[0],
                self.value[1],
                self.value[1],
                1.0 + self.value[2],
            ]
        })
    }
}

impl Scene {
    /// Gives a node a stretch, or takes it away.
    pub fn set_stretch(
        &mut self,
        node: NodeHandle,
        stretch: Option<Stretch>,
    ) -> Result<(), SceneError> {
        let id = self.live(node)?;
        match stretch {
            Some(spec) => {
                self.stretch
                    .entry(id)
                    .and_modify(|state| state.spec = spec)
                    .or_insert_with(|| StretchState::new(spec));
            }
            None => {
                self.stretch.remove(&id);
            }
        }
        Ok(())
    }

    /// How a node stretches, if it does.
    pub fn stretch(&self, node: NodeHandle) -> Option<Stretch> {
        self.stretch.get(&node.id()).map(|state| state.spec)
    }

    /// Every node that stretches.
    pub fn stretch_nodes(&self) -> Vec<NodeHandle> {
        self.stretch.keys().map(|id| NodeHandle(*id)).collect()
    }

    /// Whether any node has a stretch at all: the cheap question a frame asks
    /// before it looks at any of them.
    pub fn has_stretch(&self) -> bool {
        !self.stretch.is_empty()
    }

    /// A node's current deformation about its centre, as a linear map
    /// `[a, b, c, d]` (column major), or nothing when it has none.
    pub fn deformation(&self, node: NodeHandle) -> Option<[f64; 4]> {
        self.stretch.get(&node.id()).and_then(StretchState::matrix)
    }

    /// Tells a stretching node where its centre was drawn this frame, in
    /// surface coordinates, and how a surface-space vector maps into the
    /// frame its deformation lives in (the inverse of its ancestors' linear
    /// transform). Returns whether its deformation changed.
    ///
    /// Called once a frame per node by whoever lays the frame out; a second
    /// call on the same tick is ignored, so several surfaces may all report.
    pub fn observe_stretch(
        &mut self,
        node: NodeHandle,
        centre: [f64; 2],
        to_local: [f64; 4],
    ) -> bool {
        let now = self.stretch_clock;
        let reduced = self.motion_scale == 0.0;
        let Some(state) = self.stretch.get_mut(&node.id()) else {
            return false;
        };
        if reduced {
            let had = state.matrix().is_some();
            *state = StretchState {
                sample: Some((centre, now)),
                ..StretchState::new(state.spec)
            };
            return had;
        }
        state.observe(centre, now, to_local)
    }

    /// Whether a stretch is still settling.
    pub(crate) fn stretch_moving(&self) -> bool {
        self.stretch.values().any(|state| state.moving)
    }
}
