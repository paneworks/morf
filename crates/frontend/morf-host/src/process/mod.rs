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
    settle_allocator();
}

/// glibc's allocator at fixed thresholds: a buffer of 4 MiB or more gets
/// pages of its own, unmapped as it is freed, and an arena gives back what
/// lies free above 8 MiB at its top. A malloc tuned through the environment
/// is left as it is.
fn settle_allocator() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        let tuned = std::env::var("GLIBC_TUNABLES").is_ok_and(|t| t.contains("glibc.malloc."))
            || std::env::var_os("MALLOC_MMAP_THRESHOLD_").is_some()
            || std::env::var_os("MALLOC_TRIM_THRESHOLD_").is_some();
        if !tuned {
            // SAFETY: mallopt only sets the allocator's own parameters.
            unsafe {
                libc::mallopt(libc::M_MMAP_THRESHOLD, 4 << 20);
                libc::mallopt(libc::M_TRIM_THRESHOLD, 8 << 20);
            }
        }
    }
}
