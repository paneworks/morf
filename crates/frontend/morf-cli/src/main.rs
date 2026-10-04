use std::process::ExitCode;

mod bundle;
mod commands;
mod config;
mod runner_args;
mod runners;
mod test_host;
mod test_host_input;
mod test_runner;

use config::*;

fn main() -> ExitCode {
    // First, so a fault anywhere after this line leaves something to read.
    morf_host::crash::install();
    // The direction a layout with none of its own takes: the locale's, or
    // MORF_DIRECTION (ltr, rtl) for a run.
    morf_host::morf_scene::set_default_rtl(match std::env::var("MORF_DIRECTION").as_deref() {
        Ok("rtl") => true,
        Ok("ltr") => false,
        _ => morf_host::morf_scene::locale_rtl_from_env(),
    });
    // The widget archetypes, for every runtime this process makes.
    #[cfg(feature = "kit")]
    morf_host::morf_lua::register_kit();
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
