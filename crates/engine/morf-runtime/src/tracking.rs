//! What one turn of handlers and effects keeps count of: how many handlers
//! are on the stack, whether a flush waits for the outermost, the effect
//! whose reads are being captured, and the signal standing for each node
//! property a binding has read.

use std::collections::{HashMap, HashSet};

use morf_scene::NodeHandle;
use morf_scene::reactive::SignalId;

use crate::states::Capture;

/// A node property a binding read: the node, the property, and whether it
/// read the animation's target rather than the value on show.
pub type PropertyKey = (NodeHandle, String, bool);

#[derive(Default)]
pub struct Tracking {
    /// How many handlers are on the stack. While one runs, a write marks the
    /// graph dirty and nothing more; the one flush happens when the outermost
    /// handler returns.
    handler_depth: u32,
    /// Whether something wrote while a handler was running.
    flush_pending: bool,
    capture: Option<Capture>,
    effect_runs: u64,
    next_effect: u64,
    property_signals: HashMap<PropertyKey, SignalId>,
    /// Each current-value property signal by its graph name.
    property_names: HashMap<String, (NodeHandle, String)>,
}

impl Tracking {
    /// A handler starts.
    pub fn enter_handler(&mut self) {
        self.handler_depth += 1;
    }

    /// A handler returns; whether the flush it deferred is now due.
    pub fn leave_handler(&mut self) -> bool {
        self.handler_depth = self.handler_depth.saturating_sub(1);
        self.take_flush()
    }

    /// Whether a handler is running.
    pub fn in_handler(&self) -> bool {
        self.handler_depth > 0
    }

    /// How many handlers are on the stack.
    pub fn handler_depth(&self) -> u32 {
        self.handler_depth
    }

    /// Something wrote: flush once the outermost handler returns.
    pub fn defer_flush(&mut self) {
        self.flush_pending = true;
    }

    /// Whether a flush is waiting for the handlers to return.
    pub fn flush_pending(&self) -> bool {
        self.flush_pending
    }

    /// Forgets a deferred flush, done some other way.
    pub fn cancel_flush(&mut self) {
        self.flush_pending = false;
    }

    /// Takes the deferred flush when no handler is running.
    pub fn take_flush(&mut self) -> bool {
        self.handler_depth == 0 && std::mem::take(&mut self.flush_pending)
    }

    /// The reads and writes of the effect running now, if one is.
    pub fn capture(&mut self) -> Option<&mut Capture> {
        self.capture.as_mut()
    }

    /// Whether an effect is running.
    pub fn capturing(&self) -> bool {
        self.capture.is_some()
    }

    /// An effect starts: its reads and writes are captured from here.
    pub fn begin_capture(&mut self) {
        self.capture = Some(Capture::default());
        self.effect_runs = self.effect_runs.saturating_add(1);
    }

    /// The effect ends; what it read and wrote.
    pub fn end_capture(&mut self) -> Capture {
        self.capture.take().unwrap_or_default()
    }

    /// How many effects have run.
    pub fn effect_runs(&self) -> u64 {
        self.effect_runs
    }

    /// A fresh effect token.
    pub fn next_effect_token(&mut self) -> u64 {
        let token = self.next_effect;
        self.next_effect = self.next_effect.wrapping_add(1);
        token
    }

    /// The signal standing for a node property, once one has been read.
    pub fn property_signal(&self, key: &PropertyKey) -> Option<SignalId> {
        self.property_signals.get(key).copied()
    }

    /// Records the signal made for a property, under `name` when it stands
    /// for the value on show.
    pub fn track_property(&mut self, key: PropertyKey, signal: SignalId, name: String) {
        if !key.2 {
            self.property_names.insert(name, (key.0, key.1.clone()));
        }
        self.property_signals.insert(key, signal);
    }

    /// Every node property some binding has read.
    pub fn tracked_properties(&self) -> impl Iterator<Item = (NodeHandle, &str)> {
        self.property_signals
            .keys()
            .map(|(node, property, _)| (*node, property.as_str()))
    }

    /// The node property a current-value signal of this name stands for.
    pub fn property_named(&self, name: &str) -> Option<&(NodeHandle, String)> {
        self.property_names.get(name)
    }

    /// Forgets the property signals of removed nodes, and returns them for
    /// the graph to drop.
    pub fn forget_nodes(&mut self, removed: &HashSet<NodeHandle>) -> HashSet<SignalId> {
        let dead = self
            .property_signals
            .iter()
            .filter(|((node, _, _), _)| removed.contains(node))
            .map(|(_, signal)| *signal)
            .collect();
        self.property_signals
            .retain(|(node, _, _), _| !removed.contains(node));
        self.property_names
            .retain(|_, (node, _)| !removed.contains(node));
        dead
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flush_waits_for_the_outermost_handler() {
        let mut tracking = Tracking::default();
        tracking.enter_handler();
        tracking.enter_handler();
        tracking.defer_flush();
        assert!(!tracking.leave_handler());
        assert!(tracking.leave_handler());
        assert!(!tracking.take_flush());
    }

    #[test]
    fn a_capture_counts_its_run() {
        let mut tracking = Tracking::default();
        tracking.begin_capture();
        assert!(tracking.capturing());
        tracking.end_capture();
        assert!(!tracking.capturing());
        assert_eq!(tracking.effect_runs(), 1);
        assert_eq!(tracking.next_effect_token(), 0);
        assert_eq!(tracking.next_effect_token(), 1);
    }
}
