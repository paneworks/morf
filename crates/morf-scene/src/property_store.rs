//! Compact storage for scene values.
//!
//! Lua's reactive graph captures dependencies and dispatches bindings. The
//! scene only needs current/target values with generational handles: it never
//! registers an effect in its former Graph<Value>. Keeping that second graph
//! allocated a name, subscriber set and producer for every property slot on
//! every node, even though none could have a subscriber.

use morf_reactive::{GraphError, SignalId};
use slotmap::SlotMap;

use crate::Value;

#[derive(Default)]
pub(crate) struct PropertyStore {
    values: SlotMap<SignalId, Value>,
}

impl PropertyStore {
    pub(crate) fn signal(&mut self, _name: &'static str, value: Value) -> SignalId {
        self.values.insert(value)
    }

    pub(crate) fn read(&self, id: SignalId) -> Result<&Value, GraphError> {
        self.values.get(id).ok_or(GraphError::InvalidSignal)
    }

    pub(crate) fn write(&mut self, id: SignalId, value: Value) -> Result<bool, GraphError> {
        let slot = self.values.get_mut(id).ok_or(GraphError::InvalidSignal)?;
        let changed = *slot != value;
        if changed {
            *slot = value;
        }
        Ok(changed)
    }

    pub(crate) fn batch<R>(
        &mut self,
        writes: impl FnOnce(&mut Self) -> Result<R, GraphError>,
    ) -> Result<R, GraphError> {
        writes(self)
    }

    pub(crate) fn remove_signal(&mut self, id: SignalId) {
        self.values.remove(id);
    }

    pub(crate) fn signal_count(&self) -> usize {
        self.values.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recycled_properties_reject_the_previous_generation() {
        let mut store = PropertyStore::default();
        let old = store.signal("width", Value::Number(10.0));
        store.remove_signal(old);
        let new = store.signal("width", Value::Number(20.0));
        assert_eq!(store.read(old), Err(GraphError::InvalidSignal));
        assert_eq!(
            store.write(old, Value::Number(99.0)),
            Err(GraphError::InvalidSignal)
        );
        assert_eq!(store.read(new), Ok(&Value::Number(20.0)));
        assert_eq!(store.signal_count(), 1);
    }
}
