//! Bounded process, file, socket, and timer primitives for morf.

pub mod codec;
mod dbus_decode;
mod dbus_encode;
mod dbus_serve;
mod dbus_types;
mod files;
mod fsops;
mod http;
mod ipc;
mod process;
mod sockets;
mod streams;
mod timer;
mod wake;

pub use dbus_decode::DbusSignal;
pub use dbus_serve::{DbusCall, DbusService, NameOutcome};
pub use dbus_types::*;
pub use files::*;
pub use http::*;
pub mod fs {
    //! Filesystem operations; see [`crate::fsops`].
    pub use crate::fsops::*;
}
pub use ipc::*;
pub use process::*;
pub use sockets::*;
pub use streams::*;
pub use timer::*;
pub use wake::*;
#[cfg(test)]
mod dbus_tests;
#[cfg(test)]
mod http_tests;
#[cfg(test)]
mod tests;
