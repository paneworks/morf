//! Flushing the graph: batches of writes, and the dirty effects drained one
//! at a time in dependency depth order.

use std::collections::HashSet;

use super::*;

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

    /// The name the effect was registered under.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl<T: Clone + PartialEq + 'static> Graph<T> {
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
                written_names.push(slot.name.to_string());
            }
            self.write_from(signal, value, Some(effect))
                .map_err(|error| error.to_string())?;
        }
        Ok(written_names)
    }
}
