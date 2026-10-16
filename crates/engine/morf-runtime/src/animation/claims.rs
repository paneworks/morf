//! Claims: which state owns a property right now.
//!
//! A claim is a record: the properties it sets and their targets, how they
//! travel there, a condition, a priority class and a declared order. For
//! each claimed property the highest-class active claim wins, and inside a
//! class the claim declared first. The evaluator ([`Claims::evaluate`])
//! runs in the animation tick and only aims each property's own animation
//! at its winner's target through [`morf_scene::Scene::retarget`], which
//! carries the value and its speed across a change of motion; it moves
//! nothing itself. Properties no claim names are never touched.
//!
//! Each claim owns a presence, 0 while it yields and 1 while it owns every
//! property it sets, moved by the motion that moves those properties.

mod evaluate;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use morf_scene::reactive::SignalId;
use morf_scene::{Motion, NodeHandle, Physics, Repeat, ScalarMotion, Scene, Value};

pub use evaluate::{ClaimsFrame, ClaimsHost};

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

/// One registered claim, by the order it was registered in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClaimId(u64);

/// A claim as declared, before it is registered.
#[derive(Clone, Debug)]
pub struct ClaimSpec {
    pub name: String,
    pub class: Priority,
    /// The claim's own motion; `None` moves each property by the motion
    /// the property declares.
    pub motion: Option<Motion>,
    /// Whether its condition holds now.
    pub holds: bool,
    /// Each property it sets and the target, `None` until a target that
    /// is a binding first gives one.
    pub targets: Vec<(String, Option<Value>)>,
}

#[derive(Clone, Debug)]
struct Target {
    property: String,
    value: Option<Value>,
}

/// One registered claim.
#[derive(Clone, Debug)]
pub struct Claim {
    pub name: String,
    pub node: NodeHandle,
    pub class: Priority,
    order: u64,
    motion: Option<Motion>,
    holds: bool,
    asked: bool,
    targets: Vec<Target>,
    presence: ScalarMotion,
    presence_signal: Option<SignalId>,
    owns: bool,
    effects: Vec<u64>,
}

impl Claim {
    /// Whether its condition holds, or a handler asked for it.
    pub fn is_active(&self) -> bool {
        self.holds || self.asked
    }

    fn target(&self, property: &str) -> Option<&Value> {
        self.targets
            .iter()
            .find(|target| target.property == property)?
            .value
            .as_ref()
    }
}

/// One claimed property of a node: who claims it, who won it last, and
/// what the evaluator last aimed it at.
#[derive(Clone, Debug)]
struct Slot {
    property: String,
    claims: Vec<ClaimId>,
    winner: Option<ClaimId>,
    aimed: Option<(ClaimId, Value)>,
}

/// What a removed claim leaves for its owner to let go of.
#[derive(Debug, Default)]
pub struct Released {
    pub signals: HashSet<SignalId>,
    pub effects: Vec<u64>,
}

/// Every claim a runtime holds, indexed by node and property.
#[derive(Default)]
pub struct Claims {
    claims: HashMap<ClaimId, Claim>,
    nodes: HashMap<NodeHandle, Vec<Slot>>,
    /// Nodes whose winners have to be worked out again.
    dirty: HashSet<NodeHandle>,
    /// Claims whose presence is moving.
    moving: HashSet<ClaimId>,
    next_id: u64,
    next_order: u64,
}

impl Claims {
    /// Registers a node's claims, all of them or none: every property must
    /// be the node's, every fixed target one it takes, every motion one
    /// that can move it. Declared order is registration order, and inside
    /// one registration the claims' names in byte order. The ids come back
    /// in the order the specs were given.
    pub fn register(
        &mut self,
        scene: &Scene,
        node: NodeHandle,
        mut specs: Vec<ClaimSpec>,
    ) -> Result<Vec<ClaimId>, String> {
        if !scene.contains(node) {
            return Err("ui.claims needs a live node".to_owned());
        }
        for spec in &mut specs {
            check_spec(scene, node, spec)?;
        }
        let mut ranked = (0..specs.len()).collect::<Vec<_>>();
        ranked.sort_by(|a, b| specs[*a].name.cmp(&specs[*b].name));
        let mut rank = vec![0; specs.len()];
        for (position, index) in ranked.into_iter().enumerate() {
            rank[index] = position as u64;
        }
        let first_order = self.next_order;
        self.next_order += specs.len() as u64;
        let mut ids = Vec::with_capacity(specs.len());
        for (index, spec) in specs.into_iter().enumerate() {
            let id = ClaimId(self.next_id);
            self.next_id += 1;
            let slots = self.nodes.entry(node).or_default();
            for (property, _) in &spec.targets {
                match slots.iter_mut().find(|slot| slot.property == *property) {
                    Some(slot) => slot.claims.push(id),
                    None => slots.push(Slot {
                        property: property.clone(),
                        claims: vec![id],
                        winner: None,
                        aimed: None,
                    }),
                }
            }
            let claim = Claim {
                name: spec.name,
                node,
                class: spec.class,
                order: first_order + rank[index],
                motion: spec.motion,
                holds: spec.holds,
                asked: false,
                targets: spec
                    .targets
                    .into_iter()
                    .map(|(property, value)| Target { property, value })
                    .collect(),
                presence: ScalarMotion::new(0.0),
                presence_signal: None,
                owns: false,
                effects: Vec::new(),
            };
            self.claims.insert(id, claim);
            ids.push(id);
        }
        self.dirty.insert(node);
        Ok(ids)
    }

    /// Gives a claim the signal its presence is mirrored into and the
    /// effects that feed its condition and targets, for its removal.
    pub fn attach(&mut self, id: ClaimId, presence: Option<SignalId>, effects: Vec<u64>) {
        if let Some(claim) = self.claims.get_mut(&id) {
            claim.presence_signal = presence;
            claim.effects.extend(effects);
        }
    }

    /// Sets whether a claim's condition holds.
    pub fn set_holds(&mut self, id: ClaimId, holds: bool) {
        if let Some(claim) = self.claims.get_mut(&id)
            && claim.holds != holds
        {
            claim.holds = holds;
            self.dirty.insert(claim.node);
        }
    }

    /// Asks for an `asked` claim by hand, or lets it go.
    pub fn set_asked(&mut self, id: ClaimId, asked: bool) -> Result<(), String> {
        let claim = self
            .claims
            .get_mut(&id)
            .ok_or_else(|| "this claim was removed".to_owned())?;
        if claim.class != Priority::Asked {
            return Err(format!(
                "claim `{}` is a {} claim: only an asked claim is asked for by hand",
                claim.name,
                claim.class.name()
            ));
        }
        if claim.asked != asked {
            claim.asked = asked;
            self.dirty.insert(claim.node);
        }
        Ok(())
    }

    /// Sets the target a binding gave one of a claim's properties.
    pub fn set_target(
        &mut self,
        scene: &Scene,
        id: ClaimId,
        property: &str,
        value: Value,
    ) -> Result<(), String> {
        let claim = self
            .claims
            .get_mut(&id)
            .ok_or_else(|| "this claim was removed".to_owned())?;
        let value = scene
            .coerce_value(claim.node, property, value)
            .map_err(|error| format!("claim `{}` target `{property}`: {error}", claim.name))?;
        let target = claim
            .targets
            .iter_mut()
            .find(|target| target.property == property)
            .ok_or_else(|| format!("claim `{}` does not set `{property}`", claim.name))?;
        if target.value.as_ref() != Some(&value) {
            target.value = Some(value);
            self.dirty.insert(claim.node);
        }
        Ok(())
    }

    /// Takes a claim out. Its properties go to whoever wins them without
    /// it, and stay where they are when no claim is left on them.
    pub fn remove(&mut self, id: ClaimId) -> Option<Released> {
        let claim = self.claims.remove(&id)?;
        self.moving.remove(&id);
        if let Some(slots) = self.nodes.get_mut(&claim.node) {
            for slot in slots.iter_mut() {
                slot.claims.retain(|other| *other != id);
                if slot.winner == Some(id) {
                    slot.winner = None;
                }
            }
            slots.retain(|slot| !slot.claims.is_empty());
            if slots.is_empty() {
                self.nodes.remove(&claim.node);
                self.dirty.remove(&claim.node);
            } else {
                self.dirty.insert(claim.node);
            }
        }
        Some(Released {
            signals: claim.presence_signal.into_iter().collect(),
            effects: claim.effects,
        })
    }

    /// Lets go of every claim on nodes that were removed: the presence
    /// signals they leave, for the graph to let go of.
    pub fn forget(&mut self, removed: &HashSet<NodeHandle>) -> HashSet<SignalId> {
        let mut signals = HashSet::new();
        if self.claims.is_empty() {
            return signals;
        }
        let gone = self
            .claims
            .iter()
            .filter(|(_, claim)| removed.contains(&claim.node))
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in gone {
            if let Some(released) = self.remove(id) {
                signals.extend(released.signals);
            }
        }
        signals
    }

    /// Asks for a node's winners to be worked out again: its exit was
    /// taken back, and a deferred change may be owed.
    pub fn touch(&mut self, node: NodeHandle) {
        if self.nodes.contains_key(&node) {
            self.dirty.insert(node);
        }
    }

    /// Whether claims own `property` of `node`.
    pub fn is_claimed(&self, node: NodeHandle, property: &str) -> bool {
        self.nodes
            .get(&node)
            .is_some_and(|slots| slots.iter().any(|slot| slot.property == property))
    }

    /// Whether the next tick has work: a winner to work out or a presence
    /// on the move.
    pub fn wants_tick(&self) -> bool {
        !self.dirty.is_empty() || !self.moving.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.claims.is_empty()
    }

    /// How many claims are registered.
    pub fn len(&self) -> usize {
        self.claims.len()
    }

    pub fn claim(&self, id: ClaimId) -> Option<&Claim> {
        self.claims.get(&id)
    }

    /// The claim's presence: 0 while it yields, 1 while it owns.
    pub fn presence(&self, id: ClaimId) -> f64 {
        self.claims
            .get(&id)
            .map_or(0.0, |claim| claim.presence.value())
    }
}

/// Checks one claim against its node; a fixed target is coerced to what
/// the property holds.
fn check_spec(scene: &Scene, node: NodeHandle, spec: &mut ClaimSpec) -> Result<(), String> {
    let name = spec.name.clone();
    let fail = |message: String| format!("claim `{name}`: {message}");
    if spec.targets.is_empty() {
        return Err(fail("its `set` names no property".to_owned()));
    }
    if let Some(Motion::Tween(behavior)) = spec.motion
        && behavior.repeat != Repeat::Once
    {
        return Err(fail(
            "a claim's motion travels once; it cannot loop".to_owned(),
        ));
    }
    if let Some(Motion::Physics(Physics::Decay { .. })) = spec.motion {
        return Err(fail("a coast has no target to travel to".to_owned()));
    }
    spec.targets.sort_by(|a, b| a.0.cmp(&b.0));
    for (property, value) in &mut spec.targets {
        if !scene
            .has_property(node, property)
            .map_err(|error| fail(error.to_string()))?
        {
            let element = scene
                .element(node)
                .map_err(|error| fail(error.to_string()))?;
            return Err(fail(format!("unknown {element:?} property `{property}`")));
        }
        if let Some(motion) = spec.motion {
            scene
                .check_motion(node, property, motion)
                .map_err(|error| fail(error.to_string()))?;
        }
        if let Some(fixed) = value.take() {
            let coerced = scene
                .coerce_value(node, property, fixed)
                .map_err(|error| fail(error.to_string()))?;
            *value = Some(coerced);
        }
    }
    Ok(())
}
