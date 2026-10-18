//! A map that forgets what was used longest ago once it holds too much.
//!
//! For caches whose entries are cheap to measure again and expensive to keep
//! forever: a glyph field per letter ever drawn, a morph per pair of letters.
//! Eviction scans for the oldest entry, which is fine at these sizes because
//! anything that misses the cache costs far more to measure than the scan.

use std::hash::Hash;

use morf_scene::FastMap;

pub(crate) struct Lru<K, V> {
    entries: FastMap<K, (V, u64)>,
    clock: u64,
    capacity: usize,
}

impl<K: Copy + Eq + Hash, V> Lru<K, V> {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: FastMap::default(),
            clock: 0,
            capacity: capacity.max(1),
        }
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    pub(crate) fn contains_key(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    /// The entry, marked as just used.
    pub(crate) fn get(&mut self, key: &K) -> Option<&V> {
        self.get_mut(key).map(|value| &*value)
    }

    /// The entry, marked as just used.
    pub(crate) fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let now = self.tick();
        let (value, used) = self.entries.get_mut(key)?;
        *used = now;
        Some(value)
    }

    /// The entry, without counting it as a use.
    pub(crate) fn peek(&self, key: &K) -> Option<&V> {
        self.entries.get(key).map(|(value, _)| value)
    }

    /// Inserts, first dropping the least recently used entry when full.
    pub(crate) fn insert(&mut self, key: K, value: V) {
        if self.entries.len() >= self.capacity
            && !self.entries.contains_key(&key)
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(key, _)| *key)
        {
            self.entries.remove(&oldest);
        }
        let now = self.tick();
        self.entries.insert(key, (value, now));
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        self.entries.retain(|key, (value, _)| keep(key, value));
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::Lru;

    #[test]
    fn the_least_recently_used_entry_goes_first() {
        let mut cache = Lru::new(2);
        cache.insert(1, "a");
        cache.insert(2, "b");
        assert_eq!(cache.get(&1), Some(&"a"));
        cache.insert(3, "c");
        assert!(cache.contains_key(&1), "used since, so kept");
        assert!(!cache.contains_key(&2), "the oldest is dropped");
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn a_peek_does_not_keep_an_entry() {
        let mut cache = Lru::new(2);
        cache.insert(1, "a");
        cache.insert(2, "b");
        assert_eq!(cache.peek(&1), Some(&"a"));
        cache.insert(3, "c");
        assert!(!cache.contains_key(&1));
    }

    #[test]
    fn replacing_an_entry_never_evicts_another() {
        let mut cache = Lru::new(2);
        cache.insert(1, "a");
        cache.insert(2, "b");
        cache.insert(2, "B");
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.peek(&2), Some(&"B"));
    }
}
