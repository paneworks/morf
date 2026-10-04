use std::process::ExitCode;

mod app;
mod backdrop;
mod bundle;
mod capture;
mod commands;
mod config;
mod crash;
mod desktop;
mod headless;
mod headless_env;
mod headless_input;
mod headless_render;
mod headless_surfaces;
mod lock;
mod lock_ipc;
mod lock_outputs;
mod outputless;
mod pacing;
mod paint;
mod pointer_cursor;
mod render_target;
mod runner_args;
mod runners;
mod services;
mod socket_path;
mod supervisor;
mod surface_a11y;
mod surface_actions;
mod surface_drag;
mod surface_events;
mod surface_gesture;
mod surface_keys;
mod surface_layers;
mod surface_pointer;
mod surface_popups;
mod surface_run;
mod surface_touch;
mod surfaces;
mod test_host;
mod test_host_input;
mod test_runner;
mod wake_plan;
mod workers;

use config::*;

fn main() -> ExitCode {
    // First, so a fault anywhere after this line leaves something to read.
    crash::install();
    // The direction a layout with none of its own takes: the locale's, or
    // MORF_DIRECTION (ltr, rtl) for a run.
    morf_scene::set_default_rtl(match std::env::var("MORF_DIRECTION").as_deref() {
        Ok("rtl") => true,
        Ok("ltr") => false,
        _ => morf_scene::locale_rtl_from_env(),
    });
    // The widget archetypes, for every runtime this process makes.
    #[cfg(feature = "kit")]
    morf_kit::register();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("morf: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
