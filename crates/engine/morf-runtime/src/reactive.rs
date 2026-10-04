//! The reactive scheduler: the graph of signals and effects, the mirror of
//! signal values handlers read, the effects a configuration declared, and
//! the flush that re-runs whatever its writes made dirty.
//!
//! A flush is driven one effect at a time -- [`Reactive::begin_flush`],
//! then [`Reactive::next_effect`] and [`Reactive::complete_effect`] around
//! each effect's evaluation, then [`Reactive::finish_flush`] -- with the
//! graph left in place between the steps. An effect is a handler, and a
//! handler may create signals or effects of its own while it runs (the first
//! `require` of a module that holds state does), so the graph has to be
//! where the handler can reach it, and the caller owns the evaluation.

use std::collections::HashMap;

use morf_scene::NodeHandle;
use morf_scene::reactive::{EffectCapture, EffectId, Flush, Graph, PendingEffect, SignalId};
use morf_value::IpcValue;

use crate::Handler;

/// A node property an effect drives.
#[derive(Clone)]
pub struct PropertySink {
    pub node: NodeHandle,
    pub property: String,
}

/// What an effect's result goes to, when it is a binding.
#[derive(Clone)]
pub enum EffectSink {
    Property(PropertySink),
    State(NodeHandle),
    /// A `loop` binding: what it returns is the node's loops.
    Loop(NodeHandle),
}

/// One effect a configuration declared: a binding or a free-standing
/// effect.
#[derive(Clone)]
pub struct Effect {
    pub handler: Handler,
    pub sink: Option<EffectSink>,
    /// The node whose removal ends an effect given an owner.
    pub owner: Option<NodeHandle>,
}

/// The graph, and everything kept beside it.
#[derive(Default)]
pub struct Reactive {
    /// The graph. It stays in place through a flush; only its end takes it
    /// out for a moment.
    pub graph: Option<Graph<IpcValue>>,
    /// The mirror handlers read signals from.
    pub values: HashMap<SignalId, IpcValue>,
    pub signals: Vec<SignalId>,
    /// Signals the effects of the flush under way wrote, for the flush to
    /// confirm against the graph when it ends.
    pub flush_writes: Vec<SignalId>,
    /// The graph's handle for each effect token, so an effect can be
    /// forgotten when the node it drives is removed.
    pub effect_ids: HashMap<u64, EffectId>,
    /// Effects and signals whose owners are gone, waiting for the graph: a
    /// node removed while a flush runs is forgotten after it.
    pub dead_effects: Vec<EffectId>,
    pub dead_signals: Vec<SignalId>,
    /// Effects registered while a flush held the graph -- a binding on a
    /// node built inside an effect, or an effect made by a binding. Each is
    /// `(token, name)`, handed to the graph when the flush ends and run by
    /// the flush that follows.
    pub pending_effects: Vec<(u64, String)>,
    /// A flush is under way.
    pub flushing: bool,
    pub effects: HashMap<u64, Effect>,
}

/// A flush under way.
pub struct FlushRun {
    flush: Flush<IpcValue>,
    /// Fuel left for this flush's effects, in whatever unit the evaluator
    /// counts.
    pub remaining: u64,
}

impl Reactive {
    /// A scheduler over a fresh graph.
    pub fn new(graph: Graph<IpcValue>) -> Self {
        Self {
            graph: Some(graph),
            ..Self::default()
        }
    }

    /// Hands the graph what removed nodes left behind, when it is here to
    /// take them; while a flush runs they wait for the next call.
    pub fn collect_garbage(&mut self) {
        if self.flushing {
            return;
        }
        let Some(graph) = self.graph.as_mut() else {
            return;
        };
        for effect in self.dead_effects.drain(..) {
            graph.remove_effect(effect);
        }
        for signal in self.dead_signals.drain(..) {
            graph.remove_signal(signal);
        }
    }

    /// Hands effect `token` to the graph, or, while a flush holds it,
    /// queues it for [`Self::register_pending_effects`]. Either way the
    /// effect runs on the next flush.
    pub fn register_external_effect(&mut self, token: u64, name: String) {
        match self.graph.as_mut() {
            Some(graph) => {
                let id = graph.external_effect(name, token);
                self.effect_ids.insert(token, id);
            }
            None => self.pending_effects.push((token, name)),
        }
    }

    /// Registers the effects queued while a flush held the graph, skipping
    /// any whose owner was removed in the meantime. Returns how many were
    /// registered; nothing happens while the graph is still away.
    pub fn register_pending_effects(&mut self) -> usize {
        if self.graph.is_none() || self.pending_effects.is_empty() {
            return 0;
        }
        let pending = std::mem::take(&mut self.pending_effects);
        let mut registered = 0;
        for (token, name) in pending {
            if !self.effects.contains_key(&token) {
                continue;
            }
            self.register_external_effect(token, name);
            registered += 1;
        }
        registered
    }

    /// Starts a flush with `fuel` for its effects. Nothing to run (`None`)
    /// when one is already under way: whatever it would have run is dirty
    /// in the graph, and the flush under way drains it.
    pub fn begin_flush(&mut self, fuel: u64) -> Result<Option<FlushRun>, String> {
        if self.flushing {
            return Ok(None);
        }
        if self.graph.is_none() {
            return Err("reactive graph unavailable".to_owned());
        }
        self.flushing = true;
        Ok(Some(FlushRun {
            flush: Flush::default(),
            remaining: fuel,
        }))
    }

    /// The next effect the flush has to run, if any.
    pub fn next_effect(&mut self, run: &mut FlushRun) -> Result<Option<PendingEffect>, String> {
        self.graph
            .as_mut()
            .expect("the graph stays in place during a flush")
            .next_effect(&mut run.flush)
            .map_err(|error| error.to_string())
    }

    /// Records how an effect's run went.
    pub fn complete_effect(
        &mut self,
        run: &mut FlushRun,
        pending: PendingEffect,
        capture: EffectCapture<IpcValue>,
        outcome: Result<(), String>,
    ) {
        self.graph
            .as_mut()
            .expect("the graph stays in place during a flush")
            .complete_effect(&mut run.flush, pending, capture, outcome);
    }

    /// Ends a flush: brings the mirror in line with the graph and hands the
    /// graph its garbage. `stepped` is how stepping through the effects
    /// ended. Returns the errors the effects raised, as one message.
    pub fn finish_flush(
        &mut self,
        run: FlushRun,
        stepped: Result<(), String>,
    ) -> Result<(), String> {
        let result = stepped.map(|()| run.flush.finish());
        self.flushing = false;
        let graph = self
            .graph
            .take()
            .expect("the graph stays in place during a flush");
        // Every write mirrors itself as it is made, so a clean flush only
        // has to confirm what its effects wrote -- the graph may still have
        // refused a write the effect already mirrored. A flush that failed
        // may have rolled writes back wholesale (a loop restores every
        // original), so after one, everything is copied. Copying everything
        // after every flush cost a read and a clone per signal per flush,
        // and building a panel flushes once per binding: tens of
        // milliseconds on a panel of a thousand.
        let failed = !matches!(&result, Ok(report) if report.errors.is_empty());
        let written = std::mem::take(&mut self.flush_writes);
        let copied = if failed {
            self.signals.clone()
        } else {
            written
        };
        for signal in copied {
            if let Ok(value) = graph.read(signal) {
                self.values.insert(signal, value.clone());
            }
        }
        self.graph = Some(graph);
        match result {
            Ok(report) if report.errors.is_empty() => Ok(()),
            Ok(report) => Err(report
                .errors
                .into_iter()
                .map(|error| format!("{}: {}", error.effect, error.message))
                .collect::<Vec<_>>()
                .join("; ")),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flush_asked_for_inside_one_has_nothing_of_its_own_to_run() {
        let mut reactive = Reactive::new(Graph::default());
        let mut run = reactive
            .begin_flush(1_000)
            .unwrap()
            .expect("nothing under way");
        assert!(reactive.begin_flush(1_000).unwrap().is_none());
        assert!(reactive.next_effect(&mut run).unwrap().is_none());
        reactive.finish_flush(run, Ok(())).unwrap();
        assert!(!reactive.flushing);
        assert!(reactive.begin_flush(1_000).unwrap().is_some());
    }

    #[test]
    fn effects_queued_while_the_graph_was_away_register_only_if_still_wanted() {
        let mut reactive = Reactive::default();
        reactive.register_external_effect(1, "gone".into());
        reactive.register_external_effect(2, "kept".into());
        assert_eq!(
            reactive.register_pending_effects(),
            0,
            "the graph is still away"
        );
        reactive.graph = Some(Graph::default());
        reactive.effects.insert(
            2,
            Effect {
                handler: Handler::new(
                    crate::HandlerId(2),
                    std::rc::Rc::new(Nowhere) as std::rc::Rc<dyn crate::HandlerRegistry>,
                ),
                sink: None,
                owner: None,
            },
        );
        assert_eq!(reactive.register_pending_effects(), 1);
        assert!(reactive.effect_ids.contains_key(&2) && !reactive.effect_ids.contains_key(&1));
    }

    struct Nowhere;

    impl crate::HandlerRegistry for Nowhere {
        fn release(&self, _: crate::HandlerId) {}
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }
}
