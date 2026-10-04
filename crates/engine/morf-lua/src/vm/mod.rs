//! The Lua VM: limits, the module loader, handlers, extension points.

pub mod arguments;
pub(crate) mod config;
pub(crate) mod default;
pub(crate) mod execute;
pub(crate) mod extensions;
pub(crate) mod handler_store;
pub(crate) mod handlers;
pub(crate) mod harness;
pub(crate) mod jit;
pub(crate) mod loader;
pub mod profile;
pub(crate) mod types;
pub(crate) mod types_gen;
