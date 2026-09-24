use std::process::ExitCode;

mod backdrop;
mod bundle;
mod capture;
mod commands;
mod config;
mod crash;
mod headless;
mod headless_env;
mod headless_input;
mod headless_render;
mod headless_surfaces;
mod lock;
mod lock_ipc;
mod lock_outputs;
mod pacing;
mod paint;
mod pointer_cursor;
mod runner_args;
mod runners;
mod services;
mod socket_path;
mod supervisor;
mod surface_actions;
mod surface_drag;
mod surface_events;
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
mod workers;

use config::*;

fn main() -> ExitCode {
    // First, so a fault anywhere after this line leaves something to read.
    crash::install();
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
