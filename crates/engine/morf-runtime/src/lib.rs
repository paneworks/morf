//! What the engine *does*, with no Lua: the turn of the loop and everything
//! a configuration's handlers are called from.
//!
//! Subsystems arrive here one at a time from `morf-lua` (PLAN.md phase 5):
//! handlers, timers, the reactive scheduler, the event vocabulary, the
//! gesture recogniser and shortcuts so far.

pub mod events;
pub mod gestures;
pub mod handler;
pub mod keys;
pub mod reactive;
pub mod shortcuts;
pub mod timers;

pub use handler::{Handler, HandlerId, HandlerRegistry};
