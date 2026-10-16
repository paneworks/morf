//! Claims: which state owns a property right now (PLAN item 5).
//!
//! A claim is a record -- target property, target value, motion
//! specification, priority class, declared order -- and for each
//! claimed property the highest-class active claim wins. The evaluator
//! only ever *retargets* the property's existing animation: it installs
//! the winning claim's behaviour or physics once and writes the target
//! through `Scene::assign`, which carries the property's current value
//! and velocity into the new motion. It never integrates motion
//! itself, so there is no second animation system to disagree with the
//! scene's. Properties with no claims are never touched.
//!
//! Each claim owns a presence, 0 while it yields and 1 while it owns
//! its properties, moved over the claim's motion duration. Presence is
//! an ordinary number the scripting layer mirrors into a signal, so
//! painting "according to who is in charge" is a binding like any
//! other.

use std::collections::HashMap;
use std::time::Duration;

use morf_scene::reactive::SignalId;
use morf_scene::{Behavior, NodeHandle, Physics, Scene, SceneError, Value};

/// Priority classes, lowest first: the ordering is the semantics.
/// `state > asked > reflex > posture > ambient`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Ambient,
    Posture,
    Reflex,
    Asked,
    State,
}

impl Priority {
    pub fn parse(name: &str) -> Result<Self, String> {
        Ok(match name {
            "ambient" => Self::Ambient,
            "posture" => Self::Posture,
            "reflex" => Self::Reflex,
            "asked" => Self::Asked,
            "state" => Self::State,
            other => {
                return Err(format!(
                    "unknown claim class `{other}` (ambient, posture, reflex, asked, state)"
                ));
            }
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Ambient => "ambient",
            Self::Posture => "posture",
            Self::Reflex => "reflex",
            Self::Asked => "asked",
            Self::State => "state",
        }
    }
}

/// How a claim's target is travelled to: the property's own motion.
#[derive(Clone, Copy, Debug)]
pub enum ClaimMotion {
    Spring(Physics),
    Tween(Behavior),
    None,
}

impl ClaimMotion {
    fn presence_duration(self) -> Duration {
        match self {
            Self::Tween(behavior) => behavior.duration,
            _ => Duration::from_millis(250),
        }
    }
}

/// One registered claim. Ids are assigned by [`Claims::add`].
#[derive(Clone, Debug)]
pub struct Claim {
    /// Whoever the claim speaks for (the Lua spec's key, for messages).
    pub name: String,
    pub node: NodeHandle,
    pub property: String,
    pub target: Value,
    pub class: Priority,
    /// Declared order: inside a class, the lower order wins.
    pub order: u32,
    pub motion: ClaimMotion,
    /// Whether the claim's condition currently holds (its condition
    /// signal or its manual ask, synced by the scripting layer).
    pub active: bool,
    /// Asked by hand (`ask()` from a handler), independent of the
    /// condition signal: the claim is active while either holds.
    pub manual: bool,
    /// The reactive signal the condition is read from, if any.
    pub condition: Option<SignalId>,
    /// The reactive signal presence is mirrored into, if any.
    pub presence_signal: Option<SignalId>,
    presence: f64,
    winner: bool,
}

impl Claim {
    /// A claim that is not active yet and has no presence.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        node: NodeHandle,
        property: String,
        target: Value,
        class: Priority,
        order: u32,
        motion: ClaimMotion,
    ) -> Self {
        Self {
            name,
            node,
            property,
            target,
            class,
            order,
            motion,
            active: false,
            manual: false,
            condition: None,
            presence_signal: None,
            presence: 0.0,
            winner: false,
        }
    }
}

/// Every claim a runtime holds, and the winner last evaluated per
/// claimed property.
#[derive(Default)]
pub struct Claims {
    claims: Vec<Claim>,
    winners: HashMap<(NodeHandle, String), Option<usize>>,
}

impl Claims {
    /// Registers a claim and installs its motion on the property, so a
    /// later retarget travels the way the claim asked. The claim's
    /// motion replaces the property's previous behaviour, as any
    /// `behavior` write does.
    pub fn add(&mut self, scene: &mut Scene, claim: Claim) -> Result<usize, SceneError> {
        match claim.motion {
            ClaimMotion::Spring(physics) => {
                scene.set_physics(claim.node, &claim.property, Some(physics))?;
            }
            ClaimMotion::Tween(behavior) => {
                scene.set_behavior(claim.node, &claim.property, Some(behavior))?;
            }
            ClaimMotion::None => {}
        }
        let id = self.claims.len();
        self.claims.push(claim);
        Ok(id)
    }

    pub fn set_active(&mut self, id: usize, active: bool) {
        if let Some(claim) = self.claims.get_mut(id) {
            claim.active = active;
        }
    }

    /// Asks for a claim by hand, or lets it go: one half of its
    /// condition, the other being its condition signal.
    pub fn set_manual(&mut self, id: usize, manual: bool) {
        if let Some(claim) = self.claims.get_mut(id) {
            claim.manual = manual;
            claim.active = manual || claim.active;
        }
    }

    /// Every claim, for the scripting layer to sync conditions and
    /// mirror presences.
    pub fn claims(&self) -> &[Claim] {
        &self.claims
    }

    pub fn claims_mut(&mut self) -> &mut [Claim] {
        &mut self.claims
    }

    pub fn is_active(&self, id: usize) -> bool {
        self.claims.get(id).is_some_and(|claim| claim.active)
    }

    /// The claim's presence: 0 while it yields, 1 while it owns.
    pub fn presence(&self, id: usize) -> f64 {
        self.claims.get(id).map_or(0.0, |claim| claim.presence)
    }

    pub fn is_empty(&self) -> bool {
        self.claims.is_empty()
    }

    /// One frame of the evaluator, run inside the animation tick
    /// before the scene ticks: for each claimed property, retarget to
    /// the winning active claim's target when the winner changed, and
    /// move every claim's presence toward its goal.
    pub fn evaluate(&mut self, scene: &mut Scene, delta: Duration) -> Result<(), SceneError> {
        if self.claims.is_empty() {
            return Ok(());
        }
        // The winner per property: highest class, then lowest order.
        let mut best: HashMap<(NodeHandle, String), usize> = HashMap::new();
        for (id, claim) in self.claims.iter().enumerate() {
            if !claim.active || !scene.contains(claim.node) {
                continue;
            }
            let key = (claim.node, claim.property.clone());
            let wins = best.get(&key).is_none_or(|&current| {
                let current = &self.claims[current];
                claim.class > current.class
                    || (claim.class == current.class && claim.order < current.order)
            });
            if wins {
                best.insert(key, id);
            }
        }
        for claim in &mut self.claims {
            claim.winner = false;
        }
        for (key, id) in &best {
            self.claims[*id].winner = true;
            if self.winners.get(key) != Some(&Some(*id)) {
                let claim = &self.claims[*id];
                scene.assign(claim.node, &claim.property, claim.target.clone())?;
            }
        }
        for key in best.keys() {
            self.winners.insert(key.clone(), best.get(key).copied());
        }
        // Properties whose claims all went inactive keep their last
        // winner's target; nothing retargets.
        for (key, winner) in self.winners.iter_mut() {
            if !best.contains_key(key) {
                *winner = None;
            }
        }
        for claim in &mut self.claims {
            let goal = if claim.winner { 1.0 } else { 0.0 };
            let span = claim.motion.presence_duration().as_secs_f64();
            let step = if span > 0.0 {
                delta.as_secs_f64() / span
            } else {
                1.0
            };
            claim.presence =
                (claim.presence + (goal - claim.presence).clamp(-step, step)).clamp(0.0, 1.0);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use morf_scene::Element;

    fn scene_with_node() -> (Scene, NodeHandle) {
        let mut scene = Scene::default();
        let node = scene.create(Element::Rect);
        scene.assign(node, "x", 0.0).unwrap();
        (scene, node)
    }

    fn claim(name: &str, node: NodeHandle, target: f64, class: Priority, order: u32) -> Claim {
        Claim {
            name: name.to_owned(),
            node,
            property: "x".to_owned(),
            target: Value::Number(target),
            class,
            order,
            motion: ClaimMotion::Tween(Behavior::timed(
                Duration::from_millis(200),
                morf_scene::Easing::Linear,
            )),
            active: true,
            manual: false,
            condition: None,
            presence_signal: None,
            presence: 0.0,
            winner: false,
        }
    }

    #[test]
    fn the_highest_class_active_claim_wins_and_ties_break_by_order() {
        let (mut scene, node) = scene_with_node();
        let mut claims = Claims::default();
        let ambient = claims
            .add(
                &mut scene,
                claim("ambient", node, 10.0, Priority::Ambient, 0),
            )
            .unwrap();
        let state = claims
            .add(&mut scene, claim("state", node, 90.0, Priority::State, 5))
            .unwrap();
        claims
            .evaluate(&mut scene, Duration::from_millis(16))
            .unwrap();
        assert_eq!(scene.target(node, "x").unwrap(), &Value::Number(90.0));
        claims.set_active(state, false);
        claims
            .evaluate(&mut scene, Duration::from_millis(16))
            .unwrap();
        assert_eq!(scene.target(node, "x").unwrap(), &Value::Number(10.0));
        let _ = ambient;

        // Same class: the lower declared order wins.
        let mut claims = Claims::default();
        claims
            .add(&mut scene, claim("second", node, 50.0, Priority::Asked, 2))
            .unwrap();
        claims
            .add(&mut scene, claim("first", node, 60.0, Priority::Asked, 1))
            .unwrap();
        claims
            .evaluate(&mut scene, Duration::from_millis(16))
            .unwrap();
        assert_eq!(scene.target(node, "x").unwrap(), &Value::Number(60.0));
    }

    #[test]
    fn preemption_is_position_continuous_with_finite_velocity() {
        let (mut scene, node) = scene_with_node();
        let mut claims = Claims::default();
        claims
            .add(
                &mut scene,
                claim("ambient", node, 100.0, Priority::Ambient, 0),
            )
            .unwrap();
        let state = claims
            .add(&mut scene, {
                let mut claim = claim("state", node, 200.0, Priority::State, 0);
                claim.active = false;
                claim
            })
            .unwrap();
        for _ in 0..5 {
            claims
                .evaluate(&mut scene, Duration::from_millis(16))
                .unwrap();
            scene.tick_animations(Duration::from_millis(16)).unwrap();
        }
        let before = scene.number(node, "x").unwrap();
        assert!(before > 0.0 && before < 100.0, "mid-flight at {before}");
        claims.set_active(state, true);
        claims
            .evaluate(&mut scene, Duration::from_millis(16))
            .unwrap();
        let at_retarget = scene.number(node, "x").unwrap();
        assert_eq!(at_retarget, before, "the retarget itself moves nothing");
        scene.tick_animations(Duration::from_millis(16)).unwrap();
        let after = scene.number(node, "x").unwrap();
        assert!(
            (after - before).abs() < 20.0,
            "velocity stays finite: {before} -> {after}"
        );
        assert_eq!(scene.target(node, "x").unwrap(), &Value::Number(200.0));
    }

    #[test]
    fn presence_follows_the_winner() {
        let (mut scene, node) = scene_with_node();
        let mut claims = Claims::default();
        let ambient = claims
            .add(
                &mut scene,
                claim("ambient", node, 10.0, Priority::Ambient, 0),
            )
            .unwrap();
        for _ in 0..20 {
            claims
                .evaluate(&mut scene, Duration::from_millis(16))
                .unwrap();
        }
        assert_eq!(claims.presence(ambient), 1.0);
        claims.set_active(ambient, false);
        for _ in 0..20 {
            claims
                .evaluate(&mut scene, Duration::from_millis(16))
                .unwrap();
        }
        assert_eq!(claims.presence(ambient), 0.0);
    }

    #[test]
    fn unclaimed_properties_are_untouched() {
        let (mut scene, node) = scene_with_node();
        let mut claims = Claims::default();
        claims
            .evaluate(&mut scene, Duration::from_millis(16))
            .unwrap();
        assert_eq!(scene.target(node, "x").unwrap(), &Value::Number(0.0));
    }
}
