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
mod http_options;
mod ipc;
mod process;
mod process_view;
mod reactor;
mod reactor_core;
mod socket_view;
mod sockets;
mod streams;
mod timer;
mod wake;
mod watch;
mod watch_sysfs;

pub use dbus_decode::{DbusSignal, DbusSignalEvent};
pub use dbus_serve::{DbusCall, DbusService, NameOutcome};
pub use dbus_types::*;
pub use files::*;
pub use http::*;
pub use http_options::*;
pub mod fs {
    //! Filesystem operations: every function of the crate's `fsops`.
    pub use crate::fsops::*;
}
pub use ipc::*;
pub use process::*;
pub use process_view::*;
pub use reactor::*;
pub use socket_view::*;
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
