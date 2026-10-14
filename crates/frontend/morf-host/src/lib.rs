//! Running a configuration: its live windows on a backend, the loop that
//! turns, the lock screen, the processes around a shell (supervisor,
//! workers, services, crash reports), capture, accessibility and the
//! application mode -- and the headless runner `check`, `render` and `test`
//! stand on. The command line is `morf-cli`'s.

// The layout groups each area in a folder named for its main file
// (`paint/paint.rs`, `lock/lock.rs`), as PLAN.md lays it out.
#![allow(clippy::module_inception)]

pub mod a11y;
pub mod app;
pub mod capture;
pub mod headless;
pub mod host;
pub mod input;
pub mod lock;
pub mod paint;
pub mod process;
