//! `morf.shared(name, initial)`: one value every screen's shell sees.
//!
//! Each output runs its own copy of the configuration, so anything a copy
//! works out for itself -- a CPU sample, a GPU query, a weather answer -- is
//! worked out once per screen: three 4K screens, three `nvidia-smi` every
//! two seconds. A shared value is a signal like any other, read with `get`
//! and written with `set`, whose writes are handed to the same-named signal
//! in every other copy of the process: one copy (the primary, usually) does
//! the work and the rest read the answer.
//!
//! The values live here, outside every runtime, so they also outlive a
//! reload and hand the new copies what the old ones knew. Values are what a
//! signal holds anyway -- plain data -- so nothing but a clone crosses.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use morf_scene::reactive::SignalId;
use morf_value::IpcValue;

use crate::reactive::Reactive;

/// Moves on with every write any copy publishes.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// The last value written under each name, and the generation it was
/// written at.
static VALUES: Mutex<BTreeMap<String, (u64, IpcValue)>> = Mutex::new(BTreeMap::new());

/// One copy's view of the shared values.
#[derive(Default)]
pub struct SharedValues {
    /// Name to this copy's signal, and the generation it last took.
    by_name: HashMap<String, (SignalId, u64)>,
    by_signal: HashMap<SignalId, String>,
    /// Written here since the last turn, to publish.
    dirty: Vec<SignalId>,
    /// The generation this copy last looked at.
    seen: u64,
}

impl SharedValues {
    /// Says a signal was written; publishes it on the next turn if it is a
    /// shared one.
    pub fn note_write(&mut self, signal: SignalId) {
        if self.by_signal.contains_key(&signal) && !self.dirty.contains(&signal) {
            self.dirty.push(signal);
        }
    }
}

impl SharedValues {
    /// The signal for `name` in this copy: made on first use, seeded with what
    /// another copy (or the copy before a reload) last wrote, or `initial`.
    pub fn register(
        &mut self,
        reactive: &mut Reactive,
        name: String,
        initial: IpcValue,
    ) -> Result<SignalId, String> {
        if let Some((signal, _)) = self.by_name.get(&name) {
            return Ok(*signal);
        }
        let (value, seen) = match lock().get(&name) {
            Some((generation, value)) => (value.clone(), *generation),
            None => (initial, 0),
        };
        let signal = reactive
            .graph
            .as_mut()
            .ok_or_else(|| "reactive graph is already running".to_owned())?
            .signal(format!("shared.{name}"), value.clone());
        reactive.values.insert(signal, value);
        reactive.signals.push(signal);
        self.by_name.insert(name.clone(), (signal, seen));
        self.by_signal.insert(signal, name);
        Ok(signal)
    }

    /// Publishes what this copy wrote and takes what the others did. Returns
    /// whether any signal here changed, so the caller flushes, and whether this
    /// copy published, so the caller wakes the others (they sleep until
    /// something does).
    pub fn sync(&mut self, reactive: &mut Reactive) -> (bool, bool) {
        let dirty = std::mem::take(&mut self.dirty);
        let published = !dirty.is_empty();
        if !dirty.is_empty() {
            let mut values = lock();
            for signal in dirty {
                let (Some(name), Some(value)) =
                    (self.by_signal.get(&signal), reactive.values.get(&signal))
                else {
                    continue;
                };
                let generation = GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
                values.insert(name.clone(), (generation, value.clone()));
                if let Some(entry) = self.by_name.get_mut(name) {
                    entry.1 = generation;
                }
            }
        }
        let generation = GENERATION.load(Ordering::Acquire);
        if generation == self.seen {
            return (false, published);
        }
        self.seen = generation;
        let incoming = {
            let values = lock();
            self.by_name
                .iter_mut()
                .filter_map(|(name, (signal, seen))| {
                    let (generation, value) = values.get(name)?;
                    (*generation > *seen).then(|| {
                        *seen = *generation;
                        (*signal, value.clone())
                    })
                })
                .collect::<Vec<_>>()
        };
        let mut changed = false;
        for (signal, value) in incoming {
            if reactive.values.get(&signal) == Some(&value) {
                continue;
            }
            if let Some(graph) = reactive.graph.as_mut()
                && graph.write(signal, value.clone()).is_ok()
            {
                reactive.values.insert(signal, value);
                changed = true;
            }
        }
        (changed, published)
    }
}

fn lock() -> std::sync::MutexGuard<'static, BTreeMap<String, (u64, IpcValue)>> {
    VALUES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use morf_scene::reactive::Graph;

    fn copy() -> (Reactive, SharedValues) {
        let reactive = Reactive {
            graph: Some(Graph::default()),
            ..Reactive::default()
        };
        (reactive, SharedValues::default())
    }

    #[test]
    fn one_copy_writes_and_another_takes_it_on_its_next_sync() {
        let name = format!("test.runtime.shared.{}", std::process::id());
        let (mut a_reactive, mut a) = copy();
        let (mut b_reactive, mut b) = copy();
        let a_signal = a
            .register(&mut a_reactive, name.clone(), IpcValue::Integer(0))
            .unwrap();
        let b_signal = b
            .register(&mut b_reactive, name.clone(), IpcValue::Integer(0))
            .unwrap();
        assert_eq!(
            a.register(&mut a_reactive, name, IpcValue::Integer(9))
                .unwrap(),
            a_signal
        );
        a_reactive.values.insert(a_signal, IpcValue::Integer(7));
        a.note_write(a_signal);
        assert!(a.sync(&mut a_reactive).1, "a published");
        let (changed, published) = b.sync(&mut b_reactive);
        assert!(changed && !published);
        assert_eq!(b_reactive.values[&b_signal], IpcValue::Integer(7));
        assert_eq!(b.sync(&mut b_reactive), (false, false));
    }
}
