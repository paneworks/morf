//! `morf app <config.lua>`: a configuration run as an application rather
//! than a shell.
//!
//! One runtime -- not one per output -- whose own surface is nothing the
//! person sees: the configuration's windows are its interface
//! (`morf.window.floating`, through `lib.kit.app`), and the process ends
//! when the last of them closes. The configuration finds out it is an app
//! from `morf.capabilities().app` (and the `MORF_APP` environment).

use std::sync::atomic::{AtomicBool, Ordering};

static APP: AtomicBool = AtomicBool::new(false);

/// Runs as an application from here on.
pub(crate) fn enter() {
    APP.store(true, Ordering::SeqCst);
    // For the configuration, which reads it before anything is drawn.
    // SAFETY: set before any thread that could read the environment starts.
    unsafe { std::env::set_var("MORF_APP", "1") };
}

/// Whether this process runs an application.
pub(crate) fn is_app() -> bool {
    APP.load(Ordering::SeqCst) || std::env::var_os("MORF_APP").is_some()
}
