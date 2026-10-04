//! Data channels: typed buffers a producer writes and a node reads.
//!
//! A channel is a run of numbers -- a spectrum's bars, a history of samples
//! -- with a revision that moves on every write. A producer (the audio
//! monitor, a sampler, Lua) writes it; a `ui.Path` given `series = <id>`
//! draws it, its outline made from the numbers where it is painted, so a
//! chart that changes every frame runs no Lua to change.
//!
//! Channels live in one registry for the process: each screen's runtime,
//! its renderer and a producer thread all reach the same one by id. A
//! named channel is the same channel wherever it is asked for by name.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// The most numbers a channel holds.
pub const MAX_CHANNEL_LEN: usize = 8192;

/// One channel.
#[derive(Debug)]
pub struct Channel {
    id: u64,
    name: Option<String>,
    capacity: usize,
    /// A ring keeps the newest `capacity` numbers pushed; a frame is
    /// replaced whole by each write.
    ring: bool,
    values: Mutex<Vec<f32>>,
    revision: AtomicU64,
}

struct Registry {
    next: u64,
    by_id: HashMap<u64, Arc<Channel>>,
    by_name: HashMap<String, u64>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Mutex::new(Registry {
            next: 0,
            by_id: HashMap::new(),
            by_name: HashMap::new(),
        })
    })
}

/// Moves on every write to any channel: a loop that saw this number has
/// seen every write before it.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// The generation of all channels' writes.
pub fn channels_generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

/// A channel `capacity` long (1..=8192), a ring or a frame. A `name` gives
/// the channel already made under it, if any.
pub fn channel(name: Option<&str>, capacity: usize, ring: bool) -> Arc<Channel> {
    let capacity = capacity.clamp(1, MAX_CHANNEL_LEN);
    let mut registry = registry().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(name) = name
        && let Some(id) = registry.by_name.get(name)
        && let Some(found) = registry.by_id.get(id)
    {
        return Arc::clone(found);
    }
    registry.next += 1;
    let id = registry.next;
    let made = Arc::new(Channel {
        id,
        name: name.map(str::to_owned),
        capacity,
        ring,
        values: Mutex::new(Vec::with_capacity(capacity)),
        revision: AtomicU64::new(0),
    });
    registry.by_id.insert(id, Arc::clone(&made));
    if let Some(name) = name {
        registry.by_name.insert(name.to_owned(), id);
    }
    made
}

/// The channel numbered `id`.
pub fn channel_by_id(id: u64) -> Option<Arc<Channel>> {
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .by_id
        .get(&id)
        .cloned()
}

/// Forgets an unnamed channel; nodes still naming it draw nothing.
pub fn drop_channel(id: u64) {
    let mut registry = registry().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(channel) = registry.by_id.get(&id)
        && channel.name.is_none()
    {
        registry.by_id.remove(&id);
    }
}

impl Channel {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn is_ring(&self) -> bool {
        self.ring
    }

    /// Moves on every write to this channel.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    fn wrote(&self) {
        self.revision.fetch_add(1, Ordering::AcqRel);
        GENERATION.fetch_add(1, Ordering::AcqRel);
    }

    /// Adds one number: a ring drops its oldest past its capacity; a frame
    /// takes it as its last, up to its capacity.
    pub fn push(&self, value: f32) {
        let value = if value.is_finite() { value } else { 0.0 };
        {
            let mut values = self.values.lock().unwrap_or_else(|e| e.into_inner());
            if values.len() >= self.capacity {
                if !self.ring {
                    return;
                }
                values.remove(0);
            }
            values.push(value);
        }
        self.wrote();
    }

    /// Replaces the numbers: a ring keeps the newest `capacity` of them, a
    /// frame its first (a row of bars, left to right).
    pub fn set(&self, new: &[f32]) {
        {
            let mut values = self.values.lock().unwrap_or_else(|e| e.into_inner());
            let kept = if self.ring {
                &new[new.len().saturating_sub(self.capacity)..]
            } else {
                &new[..new.len().min(self.capacity)]
            };
            values.clear();
            values.extend(kept.iter().map(|v| if v.is_finite() { *v } else { 0.0 }));
        }
        self.wrote();
    }

    pub fn clear(&self) {
        self.values
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.wrote();
    }

    /// The numbers now, oldest first, and the revision they are at.
    pub fn snapshot(&self) -> (Vec<f32>, u64) {
        let values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        (values.clone(), self.revision())
    }

    pub fn len(&self) -> usize {
        self.values.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn last(&self) -> Option<f32> {
        self.values
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last()
            .copied()
    }

    /// The largest number, or `None` when empty.
    pub fn peak(&self) -> Option<f32> {
        self.values
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .copied()
            .reduce(f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_keep_the_newest_and_frames_are_replaced() {
        let ring = channel(None, 3, true);
        let before = channels_generation();
        for v in [1.0, 2.0, 3.0, 4.0] {
            ring.push(v);
        }
        assert_eq!(ring.snapshot().0, vec![2.0, 3.0, 4.0]);
        assert_eq!(ring.revision(), 4);
        assert!(channels_generation() >= before + 4);
        let frame = channel(None, 4, false);
        frame.set(&[0.5, f32::NAN]);
        assert_eq!(frame.snapshot().0, vec![0.5, 0.0]);
        assert_eq!(frame.peak(), Some(0.5));
        let named = channel(Some("test.channel.same"), 8, true);
        assert_eq!(
            channel(Some("test.channel.same"), 2, false).id(),
            named.id()
        );
        assert!(channel_by_id(named.id()).is_some());
    }
}
