//! The native providers prek talks to: what a shell's connection and the
//! session lock report, over one private socket transport.
//!
//! - `shell`: the shell provider and the rendered context it hands over.
//! - `lock`: the session-lock evidence.
//! - `transport`: the socket, its bounded server loop and the peer checks
//!   both share.

pub mod lock;
pub mod shell;
pub(crate) mod transport;
