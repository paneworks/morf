//! The lock screen: its surfaces, its outputs, and its control socket.

pub mod ipc;
pub mod lock;
pub mod outputs;
pub mod provider;
pub mod session;

// What the module of the same name held, where it has always been found.
pub use lock::*;
