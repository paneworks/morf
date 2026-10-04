//! Which window an event is about.

/// Surface category associated with an input event.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SurfaceRole {
    /// One wlr-layer-shell surface, addressed by its client-local identifier.
    Layer(u64),
    Popup(u64),
    Floating(u64),
    /// One ext-session-lock surface, by its index in the lock's output list
    /// (the index `SessionLockConfigure` and its kin carry).
    Lock(usize),
}
