//! Reactive signal graph for morf.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::error::Error as StdError;
use std::fmt;

use slotmap::{Key, SlotMap, new_key_type};

mod flush;

pub use flush::{Flush, PendingEffect};

new_key_type! {
    /// Generational handle to a signal slot.
    pub struct SignalId;
    /// Generational handle to a reactive effect.
    pub struct EffectId;
}

struct Signal<T> {
    /// For diagnostics only. Borrowed where it can be: a scene makes two
    /// signals for every property of every node it builds, and formatting a
    /// name for each was most of what building a node cost.
    name: Cow<'static, str>,
    value: T,
    subscribers: HashSet<EffectId>,
    producer: Option<EffectId>,
}

struct Effect {
    name: String,
    callback: EffectCallback,
    dependencies: HashSet<SignalId>,
    depth: usize,
    dirty: bool,
}

/// How one effect is evaluated.
///
/// Only one way, now. There was a second — a boxed closure owned by the graph —
/// registered by a `Graph::effect` that nothing in the workspace ever called,
/// so the variant was never constructed and the whole branch through the
/// evaluator was unreachable. It also forced a `'static` bound on `Graph<T>`
/// that only the box needed.
enum EffectCallback {
    External(u64),
}

/// Non-fatal failures reported while draining a batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectError {
    /// Name assigned when the effect was registered.
    pub effect: String,
    /// Error returned by the effect callback.
    pub message: String,
}

/// Statistics and recoverable errors from one recompute pass.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FlushReport {
    /// Number of effect evaluations performed.
    pub runs: usize,
    /// Effect failures whose staged writes were discarded.
    pub errors: Vec<EffectError>,
}

/// One effect and its currently captured signal dependencies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyEntry {
    pub effect: String,
    pub signals: Vec<String>,
    pub depth: usize,
}

/// A graph operation that cannot be recovered within the current batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphError {
    /// A stale or foreign signal handle was used.
    InvalidSignal,
    /// A stale or foreign effect handle was used.
    InvalidEffect,
    /// Effects repeatedly invalidated one another within a single batch.
    Loop { chain: Vec<String> },
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSignal => f.write_str("invalid reactive signal handle"),
            Self::InvalidEffect => f.write_str("invalid reactive effect handle"),
            Self::Loop { chain } => write!(f, "reactive loop: {}", chain.join(" -> ")),
        }
    }
}

impl StdError for GraphError {}

/// What one effect evaluation read and staged, collected apart from the graph.
///
/// Kept separate from any borrow of the graph so a host can evaluate an
/// effect while the graph stays reachable: the effect may allocate signals,
/// register further effects, or write signals it does not read, and every one
/// of those needs the graph while the evaluation is still going on.
#[derive(Debug)]
pub struct EffectCapture<T> {
    dependencies: HashSet<SignalId>,
    writes: Vec<(SignalId, T)>,
}

impl<T> Default for EffectCapture<T> {
    fn default() -> Self {
        Self {
            dependencies: HashSet::new(),
            writes: Vec::new(),
        }
    }
}

impl<T: Clone + PartialEq + 'static> EffectCapture<T> {
    /// Reads a value and captures the dependency edge.
    pub fn get(&mut self, graph: &Graph<T>, signal: SignalId) -> Result<T, GraphError> {
        if let Some((_, value)) = self.writes.iter().rev().find(|(id, _)| *id == signal) {
            self.dependencies.insert(signal);
            return Ok(value.clone());
        }
        let slot = graph.signals.get(signal).ok_or(GraphError::InvalidSignal)?;
        self.dependencies.insert(signal);
        Ok(slot.value.clone())
    }

    /// Stages a signal write which is committed only if the effect succeeds.
    pub fn set(&mut self, graph: &Graph<T>, signal: SignalId, value: T) -> Result<(), GraphError> {
        if !graph.signals.contains_key(signal) {
            return Err(GraphError::InvalidSignal);
        }
        self.writes.push((signal, value));
        Ok(())
    }
}

/// Dependency-capturing access available while an effect evaluates.
pub struct EffectContext<'a, T> {
    graph: &'a mut Graph<T>,
    capture: EffectCapture<T>,
}

impl<T: Clone + PartialEq + 'static> EffectContext<'_, T> {
    /// Allocates a signal while an external effect captures dependencies.
    pub fn signal(&mut self, name: impl Into<Cow<'static, str>>, value: T) -> SignalId {
        self.graph.signal(name, value)
    }

    /// Reads a value and captures the dependency edge.
    pub fn get(&mut self, signal: SignalId) -> Result<T, GraphError> {
        self.capture.get(self.graph, signal)
    }

    /// Stages a signal write which is committed only if the effect succeeds.
    pub fn set(&mut self, signal: SignalId, value: T) -> Result<(), GraphError> {
        self.capture.set(self.graph, signal, value)
    }
}

/// A generational signal arena with dynamic dependency capture.
pub struct Graph<T> {
    signals: SlotMap<SignalId, Signal<T>>,
    effects: SlotMap<EffectId, Effect>,
    /// Pending effects ordered by dependency depth and then their stable ID.
    /// Scanning an unordered set for every next effect made a panel-wide
    /// update quadratic in the number of bindings it woke.
    dirty: BTreeMap<(usize, u64), EffectId>,
    recompute_budget: usize,
}

impl<T: Clone + PartialEq + 'static> Default for Graph<T> {
    fn default() -> Self {
        Self::new(64)
    }
}

impl<T: Clone + PartialEq + 'static> Graph<T> {
    /// Creates a graph with a per-effect recompute budget for each batch.
    pub fn new(recompute_budget: usize) -> Self {
        Self {
            signals: SlotMap::with_key(),
            effects: SlotMap::with_key(),
            dirty: BTreeMap::new(),
            recompute_budget: recompute_budget.max(1),
        }
    }

    /// Allocates a named signal and returns its generational handle.
    pub fn signal(&mut self, name: impl Into<Cow<'static, str>>, value: T) -> SignalId {
        self.signals.insert(Signal {
            name: name.into(),
            value,
            subscribers: HashSet::new(),
            producer: None,
        })
    }

    /// Registers an externally evaluated effect identified by an opaque token.
    pub fn external_effect(&mut self, name: impl Into<String>, token: u64) -> EffectId {
        let id = self.effects.insert(Effect {
            name: name.into(),
            callback: EffectCallback::External(token),
            dependencies: HashSet::new(),
            depth: 0,
            dirty: true,
        });
        self.dirty.insert((0, id.data().as_ffi()), id);
        id
    }

    /// Forgets an effect: it stops depending on anything and never runs
    /// again. What owned it — a node's binding — is gone, and an effect
    /// left behind would keep evaluating for nothing every time a signal
    /// it once read changed.
    pub fn remove_effect(&mut self, effect: EffectId) -> bool {
        let Some(removed) = self.effects.remove(effect) else {
            return false;
        };
        self.dirty.remove(&(removed.depth, effect.data().as_ffi()));
        for signal in removed.dependencies {
            if let Some(slot) = self.signals.get_mut(signal) {
                slot.subscribers.remove(&effect);
            }
        }
        true
    }

    /// Forgets a signal. Effects that read it stop depending on it; a later
    /// read of the handle is `InvalidSignal`.
    pub fn remove_signal(&mut self, signal: SignalId) -> bool {
        let Some(removed) = self.signals.remove(signal) else {
            return false;
        };
        for effect in removed.subscribers {
            if let Some(slot) = self.effects.get_mut(effect) {
                slot.dependencies.remove(&signal);
            }
        }
        true
    }

    /// How many signals the graph holds.
    pub fn signal_count(&self) -> usize {
        self.signals.len()
    }

    /// How many effects the graph holds.
    pub fn effect_count(&self) -> usize {
        self.effects.len()
    }

    /// Whether a signal handle is still live.
    pub fn contains_signal(&self, signal: SignalId) -> bool {
        self.signals.contains_key(signal)
    }

    /// Whether any effect currently depends on a signal: whether writing it
    /// could change anything at all. A clock nobody reads need not tick.
    pub fn has_subscribers(&self, signal: SignalId) -> bool {
        self.signals
            .get(signal)
            .is_some_and(|slot| !slot.subscribers.is_empty())
    }

    /// Reads a signal without capturing a dependency.
    pub fn read(&self, signal: SignalId) -> Result<&T, GraphError> {
        self.signals
            .get(signal)
            .map(|slot| &slot.value)
            .ok_or(GraphError::InvalidSignal)
    }

    /// Returns a deterministic snapshot of the current dependency graph.
    pub fn dependencies(&self) -> Vec<DependencyEntry> {
        let mut entries = self
            .effects
            .values()
            .map(|effect| {
                let mut signals = effect
                    .dependencies
                    .iter()
                    .filter_map(|signal| self.signals.get(*signal))
                    .map(|signal| signal.name.to_string())
                    .collect::<Vec<_>>();
                signals.sort();
                DependencyEntry {
                    effect: effect.name.clone(),
                    signals,
                    depth: effect.depth,
                }
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            (left.depth, left.effect.as_str()).cmp(&(right.depth, right.effect.as_str()))
        });
        entries
    }

    /// Writes a signal and queues only effects that currently depend on it.
    pub fn write(&mut self, signal: SignalId, value: T) -> Result<bool, GraphError> {
        self.write_from(signal, value, None)
    }

    fn write_from(
        &mut self,
        signal: SignalId,
        value: T,
        producer: Option<EffectId>,
    ) -> Result<bool, GraphError> {
        let slot = self
            .signals
            .get_mut(signal)
            .ok_or(GraphError::InvalidSignal)?;
        if slot.value == value {
            if producer.is_some() {
                slot.producer = producer;
            }
            return Ok(false);
        }
        slot.value = value;
        slot.producer = producer;
        let subscribers: Vec<_> = slot.subscribers.iter().copied().collect();
        for id in subscribers {
            if let Some(effect) = self.effects.get_mut(id)
                && !effect.dirty
            {
                effect.dirty = true;
                self.dirty.insert((effect.depth, id.data().as_ffi()), id);
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
