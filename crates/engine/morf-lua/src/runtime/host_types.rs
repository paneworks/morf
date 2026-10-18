//! The window declarations and platform requests are morf-runtime's; the
//! Lua conversions of the value they carry are this crate's, by trait.

pub use morf_runtime::requests::*;
pub use morf_runtime::windows::*;

// The value that crosses the Lua boundary is morf-value's; how Lua reads
// and writes it is `crate::value::ipc`'s, by trait.
pub(crate) use crate::value::ipc::{IpcFromLua, IpcToLua};
pub(crate) use morf_value::IpcValue;
