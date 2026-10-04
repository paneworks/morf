//! What the engine *does*, with no Lua: the turn of the loop and everything
//! a configuration's handlers are called from.
//!
//! Subsystems arrive here one at a time from `morf-lua` (PLAN.md phase 5):
//! handlers, timers, the reactive scheduler, the event vocabulary, the
//! gesture recogniser, shortcuts, focus movement, wake causes, window
//! declarations, platform requests, overlays, views, text editing and motion
//! (animation) so far.

pub mod animation;
pub mod editing;
pub mod events;
pub mod focus;
pub mod gestures;
pub mod handler;
pub mod keys;
pub mod layout;
pub mod log;
pub mod models;
pub mod overlays;
pub mod reactive;
pub mod requests;
pub mod retention;
pub mod screens;
pub mod session;
pub mod shared;
pub mod shortcuts;
pub mod states;
pub mod timers;
pub mod views;
pub mod wake;
pub mod windows;

pub use handler::{Handler, HandlerId, HandlerRegistry, Handlers};
