//! Reactive signal graph for morf.

use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error as StdError;
use std::fmt;

use slotmap::{Key, SlotMap, new_key_type};

new_key_type! {
    /// Generational handle to a signal slot.
    pub struct SignalId;
    /// Generational handle to a reactive effect.
    pub struct EffectId;
}

struct Signal<T> {
    name: String,
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
    pub fn signal(&mut self, name: impl Into<String>, value: T) -> SignalId {
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

/// A flush under way, driven one effect at a time.
///
/// [`Graph::flush_external`] runs a whole flush with the graph borrowed from
/// start to finish, which means nothing evaluated inside it can reach the
/// graph — a configuration that creates a signal from inside a binding (the
/// first `require` of a module does exactly that) had nowhere to put it. A
/// host that owns the graph drives this instead: it asks for the next effect,
/// evaluates it with the graph back in its own hands, and hands the capture
/// back. Signals and effects created in between join the same flush.
#[derive(Debug)]
pub struct Flush<T> {
    report: FlushReport,
    runs: HashMap<EffectId, usize>,
    trace: VecDeque<String>,
    originals: HashMap<SignalId, (T, Option<EffectId>)>,
}

impl<T> Default for Flush<T> {
    fn default() -> Self {
        Self {
            report: FlushReport::default(),
            runs: HashMap::new(),
            trace: VecDeque::new(),
            originals: HashMap::new(),
        }
    }
}

impl<T> Flush<T> {
    /// The runs and recoverable errors of the flush so far.
    pub fn finish(self) -> FlushReport {
        self.report
    }
}

/// One effect handed out by [`Graph::next_effect`], to be evaluated and then
/// returned through [`Graph::complete_effect`].
#[derive(Debug)]
pub struct PendingEffect {
    effect: EffectId,
    token: u64,
    name: String,
    old_dependencies: HashSet<SignalId>,
}

impl PendingEffect {
    /// The opaque token the effect was registered with.
    pub fn token(&self) -> u64 {
        self.token
    }
}

/// A generational signal arena with dynamic dependency capture.
pub struct Graph<T> {
    signals: SlotMap<SignalId, Signal<T>>,
    effects: SlotMap<EffectId, Effect>,
    /// The effects waiting to run. Kept beside each effect's own flag so
    /// finding the next one to run looks at what is dirty, not at every
    /// effect the graph holds.
    dirty: HashSet<EffectId>,
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
            dirty: HashSet::new(),
            recompute_budget: recompute_budget.max(1),
        }
    }

    /// Allocates a named signal and returns its generational handle.
    pub fn signal(&mut self, name: impl Into<String>, value: T) -> SignalId {
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
        self.dirty.insert(id);
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
        self.dirty.remove(&effect);
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
                    .map(|signal| signal.name.clone())
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

    /// Applies one event's writes and drains them in a single recompute pass.
    pub fn batch(
        &mut self,
        update: impl FnOnce(&mut Self) -> Result<(), GraphError>,
    ) -> Result<FlushReport, GraphError> {
        self.batch_external(
            |token, _| Err(format!("no evaluator for external effect {token}")),
            update,
        )
    }

    /// Applies several writes as one and then drains, delegating evaluation.
    ///
    /// `batch` cannot evaluate an effect — it flushes with an evaluator that
    /// refuses every token — and since every effect in this workspace is
    /// registered externally, that made batching and effects mutually
    /// exclusive without saying so: the writes landed, the effects were marked
    /// run, and their evaluations were quietly recorded as errors.
    pub fn batch_external<F>(
        &mut self,
        evaluate: F,
        update: impl FnOnce(&mut Self) -> Result<(), GraphError>,
    ) -> Result<FlushReport, GraphError>
    where
        F: for<'a> FnMut(u64, &mut EffectContext<'a, T>) -> Result<(), String>,
    {
        update(self)?;
        self.flush_external(evaluate)
    }

    /// Drains all dirty effects in dependency depth order.
    pub fn flush(&mut self) -> Result<FlushReport, GraphError> {
        self.flush_external(|token, _| Err(format!("no evaluator for external effect {token}")))
    }

    /// Drains dirty effects while delegating externally registered evaluations.
    pub fn flush_external<F>(&mut self, mut evaluate: F) -> Result<FlushReport, GraphError>
    where
        F: for<'a> FnMut(u64, &mut EffectContext<'a, T>) -> Result<(), String>,
    {
        let mut flush = Flush::default();
        while let Some(pending) = self.next_effect(&mut flush)? {
            let mut context = EffectContext {
                graph: self,
                capture: EffectCapture::default(),
            };
            let result = evaluate(pending.token, &mut context);
            let capture = context.capture;
            self.complete_effect(&mut flush, pending, capture, result);
        }
        Ok(flush.finish())
    }

    /// Takes the next dirty effect off the flush, in dependency depth order,
    /// or nothing once every effect is clean.
    ///
    /// The effect stops depending on what it read last time until its
    /// evaluation comes back with what it reads now. An effect that has run
    /// more often than the recompute budget allows in one flush is a loop:
    /// every write the flush made is undone and the loop is reported.
    pub fn next_effect(
        &mut self,
        flush: &mut Flush<T>,
    ) -> Result<Option<PendingEffect>, GraphError> {
        let Some(effect) = self.next_dirty() else {
            return Ok(None);
        };
        let count = flush.runs.entry(effect).or_default();
        *count += 1;
        if *count > self.recompute_budget {
            for (signal, (value, producer)) in std::mem::take(&mut flush.originals) {
                if let Some(slot) = self.signals.get_mut(signal) {
                    slot.value = value;
                    slot.producer = producer;
                }
            }
            for (_, pending) in &mut self.effects {
                pending.dirty = false;
            }
            self.dirty.clear();
            return Err(GraphError::Loop {
                chain: std::mem::take(&mut flush.trace).into_iter().collect(),
            });
        }

        let name = self.effects[effect].name.clone();
        flush.trace.push_back(name.clone());
        let max_trace = self.max_trace();
        if flush.trace.len() > max_trace {
            flush.trace.pop_front();
        }
        flush.report.runs += 1;

        let slot = &mut self.effects[effect];
        slot.dirty = false;
        let old_dependencies = std::mem::take(&mut slot.dependencies);
        let EffectCallback::External(token) = slot.callback;
        self.dirty.remove(&effect);
        for signal in &old_dependencies {
            if let Some(slot) = self.signals.get_mut(*signal) {
                slot.subscribers.remove(&effect);
            }
        }
        Ok(Some(PendingEffect {
            effect,
            token,
            name,
            old_dependencies,
        }))
    }

    /// Hands an evaluation back: what it read becomes the effect's
    /// dependencies, and what it staged is written if it succeeded.
    ///
    /// A failure is recorded in the flush's report rather than ending it, and
    /// its staged writes are dropped. An effect removed while it was being
    /// evaluated — its node went away inside its own binding — keeps nothing.
    pub fn complete_effect(
        &mut self,
        flush: &mut Flush<T>,
        pending: PendingEffect,
        capture: EffectCapture<T>,
        result: Result<(), String>,
    ) {
        let PendingEffect {
            effect,
            name,
            old_dependencies,
            ..
        } = pending;
        let outcome = self.settle_effect(flush, effect, capture, old_dependencies, result);
        let max_trace = self.max_trace();
        match outcome {
            Ok(written) => flush.trace.extend(written),
            Err(message) => flush.report.errors.push(EffectError {
                effect: name,
                message,
            }),
        }
        while flush.trace.len() > max_trace {
            flush.trace.pop_front();
        }
    }

    fn max_trace(&self) -> usize {
        self.recompute_budget.saturating_mul(2).max(4)
    }

    fn next_dirty(&self) -> Option<EffectId> {
        self.dirty
            .iter()
            .filter_map(|id| Some((*id, self.effects.get(*id)?)))
            .min_by_key(|(id, effect)| (effect.depth, id.data().as_ffi()))
            .map(|(id, _)| id)
    }

    fn settle_effect(
        &mut self,
        flush: &mut Flush<T>,
        effect: EffectId,
        capture: EffectCapture<T>,
        old_dependencies: HashSet<SignalId>,
        result: Result<(), String>,
    ) -> Result<Vec<String>, String> {
        if !self.effects.contains_key(effect) {
            return result.map(|()| Vec::new());
        }
        let EffectCapture {
            dependencies,
            writes,
        } = capture;
        let dependencies = if result.is_ok() || !dependencies.is_empty() {
            dependencies
        } else {
            old_dependencies
        };
        // A signal forgotten while the effect ran is not one it can follow.
        let dependencies: HashSet<SignalId> = dependencies
            .into_iter()
            .filter(|signal| self.signals.contains_key(*signal))
            .collect();
        let depth = dependencies
            .iter()
            .filter_map(|signal| self.signals.get(*signal)?.producer)
            .filter_map(|producer| self.effects.get(producer))
            .map(|producer| producer.depth.saturating_add(1))
            .max()
            .unwrap_or(0);
        self.effects[effect].dependencies = dependencies.clone();
        self.effects[effect].depth = depth;
        for signal in dependencies {
            if let Some(slot) = self.signals.get_mut(signal) {
                slot.subscribers.insert(effect);
            }
        }

        result?;
        let mut written_names = Vec::with_capacity(writes.len());
        for (signal, value) in writes {
            if let Some(slot) = self.signals.get(signal) {
                flush
                    .originals
                    .entry(signal)
                    .or_insert_with(|| (slot.value.clone(), slot.producer));
                written_names.push(slot.name.clone());
            }
            self.write_from(signal, value, Some(effect))
                .map_err(|error| error.to_string())?;
        }
        Ok(written_names)
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
            if let Some(effect) = self.effects.get_mut(id) {
                effect.dirty = true;
                self.dirty.insert(id);
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
