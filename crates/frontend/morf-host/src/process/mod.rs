//! The processes around a shell: the supervisor, the workers per output, services, crash reports, sockets.

pub mod crash;
pub mod services;
pub mod socket_path;
pub mod supervisor;
pub mod workers;

/// The process environment the engine's libraries read, settled once.
///
/// # Safety
/// Call it first in `main`, before any thread starts.
pub unsafe fn prepare_environment() {
    unsafe { morf_audio::prepare_environment() };
}
