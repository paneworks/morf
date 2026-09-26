//! Bounded process, file, socket, and timer primitives for morf.

pub mod archive;
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
mod reactor;
mod reactor_core;
mod sockets;
mod streams;
mod timer;
mod wake;
mod watch;

pub use dbus_decode::{DbusSignal, DbusSignalEvent};
pub use dbus_serve::{DbusCall, DbusService, NameOutcome};
pub use dbus_types::*;
pub use files::*;
pub use http::*;
pub mod fs {
    //! Filesystem operations: every function of the crate's `fsops`.
    pub use crate::fsops::*;
}
pub use ipc::*;
pub use process::*;
pub use reactor::*;
pub use sockets::*;
pub use streams::*;
pub use timer::*;
pub use wake::*;
pub use watch::*;
#[cfg(test)]
mod dbus_codec_tests;
#[cfg(test)]
mod dbus_tests;
#[cfg(test)]
mod http_tests;
#[cfg(test)]
mod reactor_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod watch_tests;
