//! What a started program finds of signals: each at its default.
//!
//! A disposition set to "ignore" survives `exec`, so a shell started from a
//! background job (which ignores SIGINT and SIGQUIT) would hand that on to
//! every program it starts, and Ctrl+C in a terminal it opens would do
//! nothing. std already clears the mask and SIGPIPE; the rest are put back
//! here, between the fork and the exec.

use std::os::unix::process::CommandExt;
use std::process::Command;

const RESET: [libc::c_int; 11] = [
    libc::SIGHUP,
    libc::SIGINT,
    libc::SIGQUIT,
    libc::SIGTERM,
    libc::SIGUSR1,
    libc::SIGUSR2,
    libc::SIGALRM,
    libc::SIGCHLD,
    libc::SIGTSTP,
    libc::SIGTTIN,
    libc::SIGTTOU,
];

/// Starts `command`'s program with every signal at its default action.
pub fn default_signals(command: &mut Command) -> &mut Command {
    // SAFETY: runs in the child between fork and exec; `signal` is
    // async-signal-safe and touches nothing but the child's own table.
    unsafe {
        command.pre_exec(|| {
            for signal in RESET {
                libc::signal(signal, libc::SIG_DFL);
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ignored_interrupt_is_not_handed_on() {
        // SAFETY: this test's process only; restored before it returns.
        let before = unsafe { libc::signal(libc::SIGINT, libc::SIG_IGN) };
        let mut command = Command::new("sh");
        command.args(["-c", "kill -INT $$; sleep 5"]);
        let status = default_signals(&mut command).status().unwrap();
        // SAFETY: as above.
        unsafe { libc::signal(libc::SIGINT, before) };
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(libc::SIGINT));
    }
}
