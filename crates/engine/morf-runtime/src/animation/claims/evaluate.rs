//! The evaluator: who wins each claimed property, aiming the property's
//! own animation at the winner, and the presences that follow.

use std::time::Duration;

use morf_scene::reactive::SignalId;
use morf_scene::{Behavior, Easing, Motion, NodeHandle, Physics, Repeat, Scene, Value};

use super::{ClaimId, Claims};

/// What the evaluator asks of the runtime it lives in.
pub trait ClaimsHost {
    fn scene(&self) -> &Scene;
    /// Aims `property` of `node` at `value` by `motion` (`None`: the
    /// property's own), telling whatever reads the property.
    fn retarget(
        &mut self,
        node: NodeHandle,
        property: &str,
        value: Value,
        motion: Option<Motion>,
    ) -> Result<(), String>;
}

/// What one pass of the evaluator did.
#[derive(Debug, Default)]
pub struct ClaimsFrame {
    /// Each presence that moved, and where it is now.
    pub presences: Vec<(SignalId, f64)>,
    /// How many properties were aimed somewhere new.
    pub retargeted: usize,
    /// What could not be done, one line each; the rest went ahead.
    pub errors: Vec<String>,
    /// Whether a presence is still on the move.
    pub moving: bool,
}

/// A presence's spring settles closer than a pixel's does: it spans 0..1.
const PRESENCE_EPSILON: f64 = 1e-3;

impl Claims {
    /// One frame of the evaluator, run in the animation tick before the
    /// scene's: every node whose claims changed has its winners worked out
    /// and each property whose winner or target changed aimed at it, then
    /// every presence on the move goes on by `delta`.
    pub fn evaluate(&mut self, host: &mut dyn ClaimsHost, delta: Duration) -> ClaimsFrame {
        let mut frame = ClaimsFrame::default();
        let dirty = std::mem::take(&mut self.dirty);
        for node in dirty {
            self.evaluate_node(host, node, false, &mut frame);
        }
        let moving = self.moving.iter().copied().collect::<Vec<_>>();
        for id in moving {
            let Some(claim) = self.claims.get_mut(&id) else {
                self.moving.remove(&id);
                continue;
            };
            let still = host.scene().advance_scalar(&mut claim.presence, delta);
            if let Some(signal) = claim.presence_signal {
                frame.presences.push((signal, claim.presence.value()));
            }
            if !still {
                self.moving.remove(&id);
            }
        }
        frame.moving = !self.moving.is_empty();
        frame
    }

    /// Works out `node`'s winners now and puts each property and presence
    /// straight where it belongs, with no motion: the first values of a
    /// node's claims, as a constructor's properties are.
    pub fn settle_node(&mut self, host: &mut dyn ClaimsHost, node: NodeHandle) -> ClaimsFrame {
        let mut frame = ClaimsFrame::default();
        if self.dirty.remove(&node) {
            self.evaluate_node(host, node, true, &mut frame);
        }
        frame.moving = !self.moving.is_empty();
        frame
    }

    fn evaluate_node(
        &mut self,
        host: &mut dyn ClaimsHost,
        node: NodeHandle,
        at_once: bool,
        frame: &mut ClaimsFrame,
    ) {
        if !host.scene().contains(node) {
            return;
        }
        let Some(mut slots) = self.nodes.remove(&node) else {
            return;
        };
        for slot in &mut slots {
            let winner = slot
                .claims
                .iter()
                .filter_map(|id| Some((*id, self.claims.get(id)?)))
                .filter(|(_, claim)| claim.is_active() && claim.target(&slot.property).is_some())
                .max_by(|(_, a), (_, b)| a.class.cmp(&b.class).then(b.order.cmp(&a.order)))
                .map(|(id, _)| id);
            slot.winner = winner;
            let Some(winner) = winner else {
                continue;
            };
            let claim = &self.claims[&winner];
            let Some(target) = claim.target(&slot.property) else {
                continue;
            };
            if slot
                .aimed
                .as_ref()
                .is_some_and(|(id, value)| *id == winner && value == target)
            {
                continue;
            }
            // An exit moves the properties it names until it ends or is
            // taken back; the change waits for that.
            if exit_moves(host.scene(), node, &slot.property) {
                continue;
            }
            let motion = if at_once {
                Some(Motion::Tween(Behavior::default()))
            } else {
                claim.motion
            };
            if let Err(error) = host.retarget(node, &slot.property, target.clone(), motion) {
                frame.errors.push(format!(
                    "claim `{}` on `{}`: {error}",
                    claim.name, slot.property
                ));
            }
            frame.retargeted += 1;
            slot.aimed = Some((winner, target.clone()));
        }
        let ids = slots
            .iter()
            .flat_map(|slot| slot.claims.iter().copied())
            .collect::<std::collections::BTreeSet<_>>();
        for id in ids {
            self.update_presence(host.scene(), &slots, id, at_once, frame);
        }
        self.nodes.insert(node, slots);
    }

    /// Aims a claim's presence at 1 when it owns every property it sets
    /// and at 0 otherwise, by the motion now moving its first property.
    fn update_presence(
        &mut self,
        scene: &Scene,
        slots: &[super::Slot],
        id: ClaimId,
        at_once: bool,
        frame: &mut ClaimsFrame,
    ) {
        let Some(claim) = self.claims.get(&id) else {
            return;
        };
        let slot_of = |property: &str| slots.iter().find(|slot| slot.property == property);
        let owns = claim
            .targets
            .iter()
            .all(|target| slot_of(&target.property).is_some_and(|slot| slot.winner == Some(id)));
        if owns == claim.owns {
            return;
        }
        let first = claim.targets.first().map(|target| target.property.as_str());
        let driver = first
            .and_then(slot_of)
            .and_then(|slot| slot.winner)
            .and_then(|winner| self.claims.get(&winner))
            .unwrap_or(claim);
        let motion = if at_once {
            None
        } else {
            let declared = first.and_then(|property| scene.declared_motion(claim.node, property));
            let travel = first.and_then(|property| travel(scene, claim.node, property));
            driver
                .motion
                .or(declared)
                .and_then(|motion| presence_motion(motion, travel))
        };
        let claim = self.claims.get_mut(&id).expect("looked up above");
        claim.owns = owns;
        claim
            .presence
            .retarget(if owns { 1.0 } else { 0.0 }, motion);
        if claim.presence.is_moving() {
            self.moving.insert(id);
        } else {
            self.moving.remove(&id);
            if let Some(signal) = claim.presence_signal {
                frame.presences.push((signal, claim.presence.value()));
            }
        }
    }
}

/// Whether `node` is leaving and its exit moves `property`.
fn exit_moves(scene: &Scene, node: NodeHandle, property: &str) -> bool {
    scene.is_exiting(node)
        && scene
            .exit_spec(node)
            .is_some_and(|spec| spec.values.iter().any(|(name, _)| name == property))
}

/// How far a numeric property has left to go to its target.
fn travel(scene: &Scene, node: NodeHandle, property: &str) -> Option<f64> {
    match (
        scene.current(node, property).ok()?,
        scene.target(node, property).ok()?,
    ) {
        (Value::Number(current), Value::Number(target)) => Some((target - current).abs()),
        _ => None,
    }
}

/// The motion a presence takes from the one moving a property: the same
/// tween once through, the same spring settling on its own scale, and a
/// constant pace as long as the property's travel takes at it.
fn presence_motion(motion: Motion, travel: Option<f64>) -> Option<Motion> {
    match motion {
        Motion::Tween(behavior) => Some(Motion::Tween(Behavior {
            repeat: Repeat::Once,
            ..behavior
        })),
        Motion::Physics(Physics::Spring {
            mass,
            damping,
            stiffness,
            epsilon,
        }) => Some(Motion::Physics(Physics::Spring {
            mass,
            damping,
            stiffness,
            epsilon: epsilon.min(PRESENCE_EPSILON),
        })),
        Motion::Physics(Physics::Smoothed { velocity }) => {
            let seconds = travel? / velocity;
            (seconds.is_finite() && seconds > 0.0).then(|| {
                Motion::Tween(Behavior::timed(
                    Duration::from_secs_f64(seconds),
                    Easing::Linear,
                ))
            })
        }
        Motion::Physics(Physics::Decay { .. }) => None,
    }
}
