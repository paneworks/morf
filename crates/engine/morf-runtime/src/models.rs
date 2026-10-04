//! A list model's reads are dependencies.
//!
//! A binding that reads `model:len()` or `model:get(i)` computes from the
//! model, and has to run again when the model changes. The model is not a
//! signal -- it is a journal of rows that a repeater follows -- so each
//! model a binding has read gets one: a revision, bumped on every change.
//! The signal is made the first time a binding reads the model, so a model
//! nothing computes from costs the graph nothing.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use morf_scene::ListModel;
use morf_scene::reactive::SignalId;
use morf_value::IpcValue;

use crate::reactive::Reactive;

/// One model's revision signal.
struct ModelRevision {
    /// Held weakly, which also keeps the allocation -- and so the key --
    /// from being reused by another model while the entry exists.
    model: Weak<RefCell<ListModel>>,
    signal: SignalId,
    revision: i64,
}

/// Every list model a binding has read, by address, with its revision
/// signal.
#[derive(Default)]
pub struct ModelRevisions {
    entries: HashMap<usize, ModelRevision>,
}

fn key(model: &Rc<RefCell<ListModel>>) -> usize {
    Rc::as_ptr(model) as usize
}

impl ModelRevisions {
    /// The model's revision signal, if a binding has ever read it.
    pub fn signal(&self, model: &Rc<RefCell<ListModel>>) -> Option<SignalId> {
        self.entries.get(&key(model)).map(|entry| entry.signal)
    }

    /// The model's revision signal, made now (at revision 0) if this is the
    /// first binding to read it.
    pub fn signal_or_make(
        &mut self,
        reactive: &mut Reactive,
        model: &Rc<RefCell<ListModel>>,
    ) -> Result<SignalId, String> {
        if let Some(signal) = self.signal(model) {
            return Ok(signal);
        }
        let key = key(model);
        let value = IpcValue::Integer(0);
        let signal = reactive
            .graph
            .as_mut()
            .ok_or("reactive graph unavailable")?
            .signal(format!("list_model@{key:x}"), value.clone());
        self.entries.insert(
            key,
            ModelRevision {
                model: Rc::downgrade(model),
                signal,
                revision: 0,
            },
        );
        reactive.values.insert(signal, value);
        Ok(signal)
    }

    /// Moves a changed model's revision on, if any binding ever read it:
    /// the signal and the value to write to it.
    pub fn bump(&mut self, model: &Rc<RefCell<ListModel>>) -> Option<(SignalId, IpcValue)> {
        let entry = self.entries.get_mut(&key(model))?;
        entry.revision = entry.revision.wrapping_add(1);
        Some((entry.signal, IpcValue::Integer(entry.revision)))
    }

    /// Forgets the revision signals of models nothing holds any more.
    pub fn collect_dead(&mut self, reactive: &mut Reactive) {
        self.entries.retain(|_, entry| {
            let alive = entry.model.strong_count() > 0;
            if !alive {
                reactive.values.remove(&entry.signal);
                reactive.dead_signals.push(entry.signal);
            }
            alive
        });
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use morf_scene::reactive::Graph;

    fn reactive() -> Reactive {
        Reactive {
            graph: Some(Graph::default()),
            ..Reactive::default()
        }
    }

    #[test]
    fn a_model_gets_a_signal_once_and_moves_on_with_each_change() {
        let mut reactive = reactive();
        let mut models = ModelRevisions::default();
        let model = Rc::new(RefCell::new(ListModel::default()));
        assert_eq!(models.bump(&model), None, "nothing read it yet");
        let signal = models.signal_or_make(&mut reactive, &model).unwrap();
        assert_eq!(
            models.signal_or_make(&mut reactive, &model).unwrap(),
            signal
        );
        assert_eq!(reactive.values[&signal], IpcValue::Integer(0));
        assert_eq!(models.bump(&model), Some((signal, IpcValue::Integer(1))));
        assert_eq!(models.bump(&model), Some((signal, IpcValue::Integer(2))));
    }

    #[test]
    fn a_model_nothing_holds_gives_its_signal_back() {
        let mut reactive = reactive();
        let mut models = ModelRevisions::default();
        let model = Rc::new(RefCell::new(ListModel::default()));
        let signal = models.signal_or_make(&mut reactive, &model).unwrap();
        models.collect_dead(&mut reactive);
        assert_eq!(models.len(), 1);
        drop(model);
        models.collect_dead(&mut reactive);
        assert!(models.is_empty());
        assert!(!reactive.values.contains_key(&signal));
        assert_eq!(reactive.dead_signals, vec![signal]);
    }
}
