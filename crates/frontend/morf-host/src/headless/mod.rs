//! The headless runner: `check`, `render` and `test` stand on it.

pub mod env;
pub mod headless;
pub mod input;
pub mod lock;
pub mod render;
pub mod surfaces;

// What the module of the same name held, where it has always been found.
pub use headless::*;

#[cfg(test)]
mod live_claims;
