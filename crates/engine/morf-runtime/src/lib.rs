//! What the engine *does*, with no Lua: the turn of the loop and everything
//! a configuration's handlers are called from.
//!
//! Subsystems arrive here one at a time from `morf-lua` (PLAN.md phase 5):
//! handlers, timers and the reactive scheduler so far.

pub mod handler;
pub mod reactive;
pub mod timers;

pub use handler::{Handler, HandlerId, HandlerRegistry};
