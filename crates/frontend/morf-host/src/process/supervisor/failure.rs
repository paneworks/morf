//! How the supervisor answers a worker that failed.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// What a worker ends with when the compositor closes its surface -- which is
/// what a compositor does to every surface on an output it switches off or
/// loses, often before it says the output is gone.
pub const SURFACE_CLOSED: &str = "layer surface was closed";

/// What a worker that panicked fails with, ahead of the panic's message.
pub const PANICKED: &str = "worker panicked";

/// Closed surfaces taken in a minute before the shell stops: one closed on
/// every output the shell is given would otherwise be asked for forever.
/// Panicked workers are restarted under the same limit, counted apart.
const CLOSURES_PER_MINUTE: usize = 5;

/// What the supervisor does when a worker fails.
#[derive(Debug, PartialEq, Eq)]
pub enum FailureStep {
    /// Something is wrong with the shell: stop it.
    Stop,
    /// Its output went away: let that worker go and ask for the outputs again.
    Probe,
    /// Its worker panicked: start that output's worker again.
    Restart,
}

/// Whether a worker's failure is a caught panic.
pub fn is_panic(error: &str) -> bool {
    error.starts_with(PANICKED)
}

/// Decides a worker's failure. A closed surface is an output switched off
/// or unplugged -- the shell's last one included, when Hyprland switches to
/// its fallback output -- and the shell stopping for it left the machine
/// with no shell at all, and nothing to light a screen again. A panic
/// restarts that output alone. The caller keeps one window per kind.
pub fn failure_step(error: &str, closures: &mut VecDeque<Instant>, now: Instant) -> FailureStep {
    let step = if is_panic(error) {
        FailureStep::Restart
    } else if error == SURFACE_CLOSED {
        FailureStep::Probe
    } else {
        return FailureStep::Stop;
    };
    while closures
        .front()
        .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(60))
    {
        closures.pop_front();
    }
    closures.push_back(now);
    if closures.len() > CLOSURES_PER_MINUTE {
        FailureStep::Stop
    } else {
        step
    }
}
