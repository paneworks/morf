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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use morf_scene::reactive::SignalId;
use morf_value::IpcValue;

use crate::reactive::Reactive;

/// Wakes one copy's loop: a write it has to take is waiting.
pub type Ring = Arc<dyn Fn() + Send + Sync>;

/// Moves on with every write any copy publishes.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// The last value written under each name, and the generation it was
/// written at.
static VALUES: Mutex<BTreeMap<String, (u64, IpcValue)>> = Mutex::new(BTreeMap::new());

/// The copies that hold each name, and how to wake each: a write wakes
/// those and nobody else.
static HOLDERS: Mutex<BTreeMap<String, Vec<(u64, Ring)>>> = Mutex::new(BTreeMap::new());

/// Names each copy; 0 is a copy that has registered nothing yet.
static NEXT_COPY: AtomicU64 = AtomicU64::new(1);

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
    /// This copy's name in [`HOLDERS`].
    copy: u64,
}

impl Drop for SharedValues {
    fn drop(&mut self) {
        if self.copy == 0 {
            return;
        }
        let mut holders = holders();
        for name in self.by_name.keys() {
            if let Some(list) = holders.get_mut(name) {
                list.retain(|(copy, _)| *copy != self.copy);
                if list.is_empty() {
                    holders.remove(name);
                }
            }
        }
    }
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
    /// `ring` is what another copy's write to it calls.
    pub fn register(
        &mut self,
        reactive: &mut Reactive,
        name: String,
        initial: IpcValue,
        ring: Ring,
    ) -> Result<SignalId, String> {
        if let Some((signal, _)) = self.by_name.get(&name) {
            return Ok(*signal);
        }
        let (value, seen) = match lock().get(&name) {
            Some((generation, value)) => (value.clone(), *generation),
            None => (initial, 0),
        };
        let signal = reactive
            .signal(format!("shared.{name}"), value)
            .ok_or(crate::reactive::GRAPH_RUNNING)?;
        if self.copy == 0 {
            self.copy = NEXT_COPY.fetch_add(1, Ordering::Relaxed);
        }
        holders()
            .entry(name.clone())
            .or_default()
            .push((self.copy, ring));
        self.by_name.insert(name.clone(), (signal, seen));
        self.by_signal.insert(signal, name);
        Ok(signal)
    }

    /// Publishes what this copy wrote and takes what the others did. Returns
    /// whether any signal here changed, so the caller flushes, and whether this
    /// copy published. A write wakes the other copies holding its name, which
    /// sleep until something does, and no other.
    pub fn sync(&mut self, reactive: &mut Reactive) -> (bool, bool) {
        let dirty = std::mem::take(&mut self.dirty);
        let published = !dirty.is_empty();
        if !dirty.is_empty() {
            let mut written = Vec::new();
            let mut values = lock();
            for signal in dirty {
                let (Some(name), Some(value)) =
                    (self.by_signal.get(&signal), reactive.value(signal))
                else {
                    continue;
                };
                let generation = GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
                values.insert(name.clone(), (generation, value.clone()));
                if let Some(entry) = self.by_name.get_mut(name) {
                    entry.1 = generation;
                }
                written.push(name.clone());
            }
            drop(values);
            self.ring_holders(&written);
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
            changed |= reactive.update(signal, value);
        }
        (changed, published)
    }

    /// Wakes every other copy holding one of `names`, each once.
    fn ring_holders(&self, names: &[String]) {
        let mut rings: Vec<(u64, Ring)> = Vec::new();
        {
            let holders = holders();
            for list in names.iter().filter_map(|name| holders.get(name)) {
                for (copy, ring) in list {
                    if *copy != self.copy && !rings.iter().any(|(held, _)| held == copy) {
                        rings.push((*copy, Arc::clone(ring)));
                    }
                }
            }
        }
        for (_, ring) in rings {
            ring();
        }
    }
}

fn lock() -> std::sync::MutexGuard<'static, BTreeMap<String, (u64, IpcValue)>> {
    VALUES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn holders() -> std::sync::MutexGuard<'static, BTreeMap<String, Vec<(u64, Ring)>>> {
    HOLDERS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use morf_scene::reactive::Graph;

    use std::sync::atomic::AtomicUsize;

    fn copy() -> (Reactive, SharedValues) {
        let reactive = Reactive::new(Graph::default());
        (reactive, SharedValues::default())
    }

    /// A ring that counts, standing in for one loop's alarm.
    fn counter() -> (Ring, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        let held = Arc::clone(&count);
        let ring: Ring = Arc::new(move || {
            held.fetch_add(1, Ordering::Relaxed);
        });
        (ring, count)
    }

    fn quiet() -> Ring {
        counter().0
    }

    #[test]
    fn one_copy_writes_and_another_takes_it_on_its_next_sync() {
        let name = format!("test.runtime.shared.{}", std::process::id());
        let (mut a_reactive, mut a) = copy();
        let (mut b_reactive, mut b) = copy();
        let a_signal = a
            .register(&mut a_reactive, name.clone(), IpcValue::Integer(0), quiet())
            .unwrap();
        let b_signal = b
            .register(&mut b_reactive, name.clone(), IpcValue::Integer(0), quiet())
            .unwrap();
        assert_eq!(
            a.register(&mut a_reactive, name, IpcValue::Integer(9), quiet())
                .unwrap(),
            a_signal
        );
        a_reactive.mirror(a_signal, IpcValue::Integer(7));
        a.note_write(a_signal);
        assert!(a.sync(&mut a_reactive).1, "a published");
        let (changed, published) = b.sync(&mut b_reactive);
        assert!(changed && !published);
        assert_eq!(b_reactive.value(b_signal), Some(&IpcValue::Integer(7)));
        assert_eq!(b.sync(&mut b_reactive), (false, false));
    }

    #[test]
    fn a_write_wakes_only_the_other_copies_holding_that_name() {
        let name = format!("test.runtime.shared.wake.{}", std::process::id());
        let other = format!("{name}.other");
        let (mut a_reactive, mut a) = copy();
        let (mut b_reactive, mut b) = copy();
        let (mut c_reactive, mut c) = copy();
        let (a_ring, a_count) = counter();
        let (b_ring, b_count) = counter();
        let (c_ring, c_count) = counter();
        let a_signal = a
            .register(&mut a_reactive, name.clone(), IpcValue::Integer(0), a_ring)
            .unwrap();
        b.register(&mut b_reactive, name.clone(), IpcValue::Integer(0), b_ring)
            .unwrap();
        c.register(&mut c_reactive, other, IpcValue::Integer(0), c_ring)
            .unwrap();
        a_reactive.mirror(a_signal, IpcValue::Integer(1));
        a.note_write(a_signal);
        a.sync(&mut a_reactive);
        let counts = || [&a_count, &b_count, &c_count].map(|count| count.load(Ordering::Relaxed));
        assert_eq!(counts(), [0, 1, 0], "the writer and the stranger slept");
        drop(b);
        a_reactive.mirror(a_signal, IpcValue::Integer(2));
        a.note_write(a_signal);
        a.sync(&mut a_reactive);
        assert_eq!(counts(), [0, 1, 0], "a copy that is gone is not rung");
    }
}
