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

use crate::state::ReactiveState;
use crate::surface_types::IpcValue;
use crate::types::Runtime;

/// Moves on with every write any copy publishes.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// The last value written under each name, and the generation it was
/// written at.
static VALUES: Mutex<BTreeMap<String, (u64, IpcValue)>> = Mutex::new(BTreeMap::new());

/// One copy's view of the shared values.
#[derive(Default)]
pub(crate) struct SharedValues {
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
    pub(crate) fn note_write(&mut self, signal: SignalId) {
        if self.by_signal.contains_key(&signal) && !self.dirty.contains(&signal) {
            self.dirty.push(signal);
        }
    }
}

/// The signal for `name` in this copy: made on first use, seeded with what
/// another copy (or the copy before a reload) last wrote, or `initial`.
pub(crate) fn register(
    state: &mut ReactiveState,
    name: String,
    initial: IpcValue,
) -> Result<SignalId, String> {
    crate::runtime_helpers::validate_scope_part(&name)?;
    if let Some((signal, _)) = state.shared.by_name.get(&name) {
        return Ok(*signal);
    }
    let (value, seen) = match lock().get(&name) {
        Some((generation, value)) => (value.clone(), *generation),
        None => (initial, 0),
    };
    let signal = state
        .reactive
        .graph
        .as_mut()
        .ok_or_else(|| "reactive graph is already running".to_owned())?
        .signal(format!("shared.{name}"), value.clone());
    state.reactive.values.insert(signal, value);
    state.reactive.signals.push(signal);
    state.shared.by_name.insert(name.clone(), (signal, seen));
    state.shared.by_signal.insert(signal, name);
    Ok(signal)
}

/// Publishes what this copy wrote and takes what the others did. Returns
/// whether any signal here changed, so the caller flushes.
pub(crate) fn sync(state: &mut ReactiveState) -> bool {
    let dirty = std::mem::take(&mut state.shared.dirty);
    if !dirty.is_empty() {
        let mut values = lock();
        for signal in dirty {
            let (Some(name), Some(value)) = (
                state.shared.by_signal.get(&signal),
                state.reactive.values.get(&signal),
            ) else {
                continue;
            };
            let generation = GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
            values.insert(name.clone(), (generation, value.clone()));
            if let Some(entry) = state.shared.by_name.get_mut(name) {
                entry.1 = generation;
            }
        }
        drop(values);
        // The other copies sleep until something wakes them.
        morf_io::wake_all();
    }
    let generation = GENERATION.load(Ordering::Acquire);
    if generation == state.shared.seen {
        return false;
    }
    state.shared.seen = generation;
    let incoming = {
        let values = lock();
        state
            .shared
            .by_name
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
        if state.reactive.values.get(&signal) == Some(&value) {
            continue;
        }
        if let Some(graph) = state.reactive.graph.as_mut()
            && graph.write(signal, value.clone()).is_ok()
        {
            state.reactive.values.insert(signal, value);
            changed = true;
        }
    }
    changed
}

fn lock() -> std::sync::MutexGuard<'static, BTreeMap<String, (u64, IpcValue)>> {
    VALUES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Runtime {
    /// [`sync`], then the bindings that read what came in.
    pub(crate) fn poll_shared(&mut self) -> bool {
        if !sync(&mut self.reactive.borrow_mut()) {
            return false;
        }
        let limits = self.limits;
        let reactive = std::rc::Rc::clone(&self.reactive);
        self.lua.enter(|ctx| {
            if let Err(message) = crate::reactive_bindings::flush_reactive(&reactive, ctx, limits) {
                reactive
                    .borrow_mut()
                    .log(crate::LogLevel::Warn, format!("shared value: {message}"));
            }
        });
        true
    }
}
