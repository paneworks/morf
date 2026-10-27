//! What a runtime holds open, counted: for the tests and diagnostics that
//! check a binding gave back everything it took.

use crate::Runtime;

/// What a runtime holds open right now.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceCounts {
    /// `morf.dbus` signal subscriptions, name watches included.
    pub dbus_subscriptions: usize,
    /// `morf.dbus` calls waiting for an answer.
    pub dbus_calls: usize,
    /// Timers, one-shot and repeating, still due.
    pub timers: usize,
    /// Children of `morf.process.run` and `morf.process.spawn` still held.
    pub processes: usize,
    /// `morf.socket.connect` connections still open.
    pub connections: usize,
    /// `morf.fs.watch` watches still open.
    pub file_watches: usize,
    /// Teardowns waiting for the nodes that own them (`morf.service`'s
    /// `owner`).
    pub owned_hooks: usize,
}

impl Runtime {
    /// What this runtime holds open right now.
    pub fn resource_counts(&self) -> ResourceCounts {
        let state = self.reactive.borrow();
        ResourceCounts {
            dbus_subscriptions: state.dbus().subscription_count(),
            dbus_calls: state.dbus().reply_count(),
            timers: state.timers().len(),
            processes: state.io().processes(),
            connections: state.io().connections(),
            file_watches: state.watches().len(),
            owned_hooks: state.destroy_hooks().owned_count(),
        }
    }
}
