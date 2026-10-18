//! What a started program finds of signals: each at its default.
//!
//! A disposition set to "ignore" survives `exec`, so a shell started from a
//! background job (which ignores SIGINT and SIGQUIT) would hand that on to
//! every program it starts, and Ctrl+C in a terminal it opens would do
//! nothing. morf catches none of these, so the ignored ones are put back
//! once at start. A `pre_exec` hook would do it per child, but std then
//! forks the whole process instead of using `posix_spawn`, and a fork of a
//! shell with a GPU device and a large heap costs every spawn milliseconds.

const RESET: [libc::c_int; 10] = [
    libc::SIGHUP,
    libc::SIGINT,
    libc::SIGQUIT,
    libc::SIGTERM,
    libc::SIGUSR1,
    libc::SIGUSR2,
    libc::SIGALRM,
    libc::SIGTSTP,
    libc::SIGTTIN,
    libc::SIGTTOU,
];

/// Puts back the default action of every signal in `RESET` this process
/// was started ignoring. Call before any thread or child is started.
pub fn restore_ignored_signals() {
    for signal in RESET {
        // SAFETY: reads and writes this process's own table; `sigaction`
        // with a null new action only reads.
        unsafe {
            let mut current: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(signal, std::ptr::null(), &mut current) == 0
                && current.sa_sigaction == libc::SIG_IGN
            {
                libc::signal(signal, libc::SIG_DFL);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::Command;

    #[test]
    fn an_ignored_interrupt_is_not_handed_on() {
        // SAFETY: this test's process only; restored before it returns.
        let before = unsafe { libc::signal(libc::SIGINT, libc::SIG_IGN) };
        restore_ignored_signals();
        let status = Command::new("sh")
            .args(["-c", "kill -INT $$; sleep 5"])
            .status()
            .unwrap();
        // SAFETY: as above.
        unsafe { libc::signal(libc::SIGINT, before) };
        assert_eq!(status.signal(), Some(libc::SIGINT));
    }
}
