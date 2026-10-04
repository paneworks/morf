//! Running a configuration: its live windows on a backend, the loop that
//! turns, the lock screen, the processes around a shell (supervisor,
//! workers, services, crash reports), capture, accessibility and the
//! application mode -- and the headless runner `check`, `render` and `test`
//! stand on. The command line is `morf-cli`'s.

// The engine crates the command line drives a host with, so it names only
// this one.
pub use {morf_app, morf_io, morf_lua, morf_scene, morf_text};

pub mod a11y;
pub mod app;
pub mod capture;
pub mod headless;
pub mod host;
pub mod input;
pub mod lock;
pub mod paint;
pub mod process;

// Every module under the name it had before the directories: paths
// throughout this crate and the command line keep working.
pub use self::a11y::surface as surface_a11y;
pub use self::headless::env as headless_env;
pub use self::headless::input as headless_input;
pub use self::headless::render as headless_render;
pub use self::headless::surfaces as headless_surfaces;
pub use self::host::actions as surface_actions;
pub use self::host::desktop;
pub use self::host::events as surface_events;
pub use self::host::layers as surface_layers;
pub use self::host::popups as surface_popups;
pub use self::host::run as surface_run;
pub use self::host::surfaces;
pub use self::input::cursor as pointer_cursor;
pub use self::input::drag as surface_drag;
pub use self::input::gesture as surface_gesture;
pub use self::input::keys as surface_keys;
pub use self::input::pointer as surface_pointer;
pub use self::input::touch as surface_touch;
pub use self::lock::ipc as lock_ipc;
pub use self::lock::outputs as lock_outputs;
pub use self::paint::backdrop;
pub use self::paint::outputless;
pub use self::paint::pacing;
pub use self::paint::render_target;
pub use self::paint::wake_plan;
pub use self::process::crash;
pub use self::process::services;
pub use self::process::socket_path;
pub use self::process::supervisor;
pub use self::process::workers;
