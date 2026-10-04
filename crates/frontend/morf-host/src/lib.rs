//! Running a configuration: its live windows on a backend, the loop that
//! turns, the lock screen, the processes around a shell (supervisor,
//! workers, services, crash reports), capture, accessibility and the
//! application mode -- and the headless runner `check`, `render` and `test`
//! stand on. The command line is `morf-cli`'s.

pub mod app;
pub mod backdrop;
pub mod capture;
pub mod crash;
pub mod desktop;
pub mod headless;
pub mod headless_env;
pub mod headless_input;
pub mod headless_render;
pub mod headless_surfaces;
pub mod lock;
pub mod lock_ipc;
pub mod lock_outputs;
pub mod outputless;
pub mod pacing;
pub mod paint;
pub mod pointer_cursor;
pub mod render_target;
pub mod services;
pub mod socket_path;
pub mod supervisor;
pub mod surface_a11y;
pub mod surface_actions;
pub mod surface_drag;
pub mod surface_events;
pub mod surface_gesture;
pub mod surface_keys;
pub mod surface_layers;
pub mod surface_pointer;
pub mod surface_popups;
pub mod surface_run;
pub mod surface_touch;
pub mod surfaces;
pub mod wake_plan;
pub mod workers;
