//! Which window an event is about.

/// A window, by its kind and the number the host gave it (a lock surface:
/// by its output's index).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WindowId {
    /// A layer surface (a bar, a dock, an overlay), by the host's number.
    Layer(u64),
    /// A popup, by the host's number.
    Popup(u64),
    /// A toplevel, by the host's number: the compositor decides whether it
    /// floats or tiles.
    Toplevel(u64),
    /// A lock surface, by its index in the lock's output list (the index
    /// `SessionLockConfigure` and its kin carry).
    Lock(usize),
}
