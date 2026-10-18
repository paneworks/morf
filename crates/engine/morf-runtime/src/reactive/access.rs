//! Signals and effects as the rest of the engine reaches them: made,
//! written and read through the mirror, never by touching the graph.

use std::borrow::Cow;

use morf_scene::reactive::{Graph, SignalId};
use morf_value::IpcValue;

use super::{Effect, Reactive};
use crate::effect_errors::EffectErrors;

/// What a write fails with while a flush holds the graph.
pub const GRAPH_RUNNING: &str = "reactive graph is already running";

impl Reactive {
    /// The graph, unless a flush holds it.
    pub fn graph(&self) -> Option<&Graph<IpcValue>> {
        self.graph.as_ref()
    }

    /// Makes a signal holding `value`, mirrored and copied back after a
    /// failed flush. `None` while a flush holds the graph.
    pub fn signal(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        value: IpcValue,
    ) -> Option<SignalId> {
        let signal = self.graph.as_mut()?.signal(name, value.clone());
        self.adopt(signal, value);
        Some(signal)
    }

    /// Mirrors and tracks a signal made on the graph before it was handed
    /// over.
    pub fn adopt(&mut self, signal: SignalId, value: IpcValue) {
        self.values.insert(signal, value);
        self.signals.push(signal);
    }

    /// What `signal` holds, as last mirrored.
    pub fn value(&self, signal: SignalId) -> Option<&IpcValue> {
        self.values.get(&signal)
    }

    /// Writes `signal` through the graph and mirrors it.
    pub fn write(&mut self, signal: SignalId, value: IpcValue) -> Result<(), String> {
        self.graph
            .as_mut()
            .ok_or(GRAPH_RUNNING)?
            .write(signal, value.clone())
            .map_err(|error| error.to_string())?;
        self.values.insert(signal, value);
        Ok(())
    }

    /// Writes `signal` when the graph is in place and takes the write;
    /// whether it did.
    pub fn try_write(&mut self, signal: SignalId, value: IpcValue) -> bool {
        let written = self
            .graph
            .as_mut()
            .is_some_and(|graph| graph.write(signal, value.clone()).is_ok());
        if written {
            self.values.insert(signal, value);
        }
        written
    }

    /// Writes `signal` unless it holds `value` already; whether it did.
    pub fn update(&mut self, signal: SignalId, value: IpcValue) -> bool {
        self.values.get(&signal) != Some(&value) && self.try_write(signal, value)
    }

    /// Writes `signal` through the graph when it is in place, and mirrors
    /// `value` either way; the error is the graph's.
    pub fn force(&mut self, signal: SignalId, value: IpcValue) -> Result<(), String> {
        let written = match self.graph.as_mut() {
            Some(graph) => graph
                .write(signal, value.clone())
                .map(|_| ())
                .map_err(|error| error.to_string()),
            None => Ok(()),
        };
        self.values.insert(signal, value);
        written
    }

    /// Mirrors `value` without writing the graph: a write captured for an
    /// effect, which the graph takes when the effect completes.
    pub fn mirror(&mut self, signal: SignalId, value: IpcValue) {
        self.values.insert(signal, value);
    }

    /// Mirrors what an effect of the flush under way wrote, for the flush
    /// to confirm when it ends.
    pub fn mirror_flush_write(&mut self, signal: SignalId, value: IpcValue) {
        self.values.insert(signal, value);
        self.flush_writes.push(signal);
    }

    /// Signals waiting for the graph to let go of them.
    #[cfg(test)]
    pub(crate) fn dead_signals(&self) -> &[SignalId] {
        &self.dead_signals
    }

    /// How many signals are mirrored and tracked.
    pub fn signal_count(&self) -> usize {
        self.signals.len()
    }

    /// Whether a flush is under way.
    pub fn is_flushing(&self) -> bool {
        self.flushing
    }

    /// Declares effect `token`.
    pub fn add_effect(&mut self, token: u64, effect: Effect) {
        self.effects.insert(token, effect);
    }

    /// Effect `token`, while it is declared.
    pub fn effect(&self, token: u64) -> Option<&Effect> {
        self.effects.get(&token)
    }

    /// Every declared effect, by token.
    pub fn effects(&self) -> impl Iterator<Item = (u64, &Effect)> {
        self.effects.iter().map(|(token, effect)| (*token, effect))
    }

    /// How many effects are declared.
    pub fn effect_count(&self) -> usize {
        self.effects.len()
    }

    /// What the effects raised.
    pub fn errors(&self) -> &EffectErrors {
        &self.errors
    }

    pub fn errors_mut(&mut self) -> &mut EffectErrors {
        &mut self.errors
    }
}
