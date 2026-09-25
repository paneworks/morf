//! A child on a pseudo-terminal, watched by morf's reactor.
//!
//! The child gets the terminal's slave side as its stdin, stdout and stderr,
//! a session of its own with that terminal as its controlling tty, and the
//! environment a terminal program expects. The master side is handed to the
//! [`Reactor`] twice — once as the output it reads, once as the input it
//! writes — so reading, back pressure, queued writes and the reap are the
//! ones every other child gets, on the thread every other child is watched
//! from. Nothing here polls and nothing here has a thread.
//!
//! One more copy of the master stays here, for the one thing only a terminal
//! does: being told its size. A resize is `TIOCSWINSZ` on it, and the kernel
//! sends `SIGWINCH` to whatever is in the foreground.

use std::collections::BTreeMap;
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{ChildStdin, ChildStdout, Command, Stdio};

use morf_io::{IoHandle, OutputMode, Reactor, ReactorControl, SpawnOptions, StdinMode};
use rustix::pty::{OpenptFlags, grantpt, openpt, unlockpt};
use rustix::termios::{InputModes, OptionalActions, Winsize, tcgetattr, tcsetattr, tcsetwinsize};

/// How to start a program on a terminal.
#[derive(Clone, Debug, Default)]
pub struct PtyOptions {
    /// The argv; never a shell unless it names one.
    pub command: Vec<String>,
    /// Set in the child on top of what it inherits and what a terminal sets.
    pub environment: BTreeMap<String, String>,
    pub working_directory: Option<PathBuf>,
    pub size: PtySize,
}

/// A terminal's size, in cells and in pixels.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PtySize {
    pub columns: u16,
    pub rows: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl PtySize {
    fn winsize(self) -> Winsize {
        Winsize {
            ws_row: self.rows.max(1),
            ws_col: self.columns.max(1),
            ws_xpixel: self.columns.saturating_mul(self.cell_width),
            ws_ypixel: self.rows.saturating_mul(self.cell_height),
        }
    }
}

/// What a terminal program is told it is running in, whatever morf itself
/// was started under. The configuration's own `env` still has the last word.
fn terminal_environment(command: &mut Command) {
    // morf may run under a wrapper that points LD_LIBRARY_PATH at its own
    // libraries (nixGL, a bundle); a system program that inherits them loads
    // the wrong ones and fails in ways that look like its own fault.
    command.env_remove("LD_LIBRARY_PATH");
    // A size inherited from whatever started morf would be wrong here; the
    // program asks the terminal instead.
    command.env_remove("COLUMNS");
    command.env_remove("LINES");
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "morf");
}

/// A program running on a pseudo-terminal.
pub struct Pty {
    master: OwnedFd,
    handle: IoHandle,
    control: ReactorControl,
    closed: bool,
}

impl Pty {
    /// Opens a terminal, starts `options.command` on it, and gives its
    /// master side to `reactor`. Output arrives as [`morf_io::IoEvent::Stdout`]
    /// chunks for [`Pty::handle`], and the end as [`morf_io::IoEvent::Exit`].
    pub fn spawn(reactor: &Reactor, options: &PtyOptions) -> io::Result<Self> {
        let (program, args) = options.command.split_first().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "command cannot be empty")
        })?;
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC)?;
        grantpt(&master)?;
        unlockpt(&master)?;
        let slave = open_slave(&master)?;
        // Line editing that knows a character may be several bytes, so a
        // shell's Backspace takes back all of one.
        if let Ok(mut termios) = tcgetattr(&slave) {
            termios.input_modes |= InputModes::IUTF8;
            let _ = tcsetattr(&slave, OptionalActions::Now, &termios);
        }
        tcsetwinsize(&master, options.size.winsize())?;

        let child = {
            let mut command = Command::new(program);
            command.args(args);
            terminal_environment(&mut command);
            command.envs(&options.environment);
            if let Some(directory) = &options.working_directory {
                command.current_dir(directory);
            }
            command
                .stdin(Stdio::from(slave.try_clone()?))
                .stdout(Stdio::from(slave.try_clone()?))
                .stderr(Stdio::from(slave));
            // SAFETY: only async-signal-safe calls, between fork and exec.
            unsafe {
                command.pre_exec(|| {
                    // A session of its own, with this terminal as its
                    // controlling tty: job control, ^C and ^Z, and the
                    // hang-up when the terminal goes, all work as they do
                    // in any terminal.
                    if libc::setsid() < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    // What morf ignores or blocks for itself is not the
                    // program's business.
                    for signal in [
                        libc::SIGCHLD,
                        libc::SIGHUP,
                        libc::SIGINT,
                        libc::SIGQUIT,
                        libc::SIGTERM,
                        libc::SIGALRM,
                        libc::SIGTSTP,
                        libc::SIGTTIN,
                        libc::SIGTTOU,
                        libc::SIGPIPE,
                    ] {
                        libc::signal(signal, libc::SIG_DFL);
                    }
                    let mut empty: libc::sigset_t = std::mem::zeroed();
                    libc::sigemptyset(&mut empty);
                    libc::sigprocmask(libc::SIG_SETMASK, &empty, std::ptr::null_mut());
                    Ok(())
                });
            }
            // The command, and the slave copies it holds, are dropped at the
            // end of this block: the child's end of the terminal must be the
            // child's alone, or its exit is never seen as the terminal closing.
            command.spawn()?
        };
        let mut child = child;
        child.stdout = Some(ChildStdout::from(master.try_clone()?));
        child.stdin = Some(ChildStdin::from(master.try_clone()?));
        child.stderr = None;
        let mut spawn = SpawnOptions::new(options.command.clone());
        spawn.stdin = StdinMode::Pipe;
        spawn.stdout = OutputMode::Capture;
        spawn.stderr = OutputMode::Null;
        spawn.lines = false;
        let handle = reactor.adopt(child, spawn);
        Ok(Self {
            master,
            handle,
            control: reactor.control(),
            closed: false,
        })
    }

    /// The reactor's handle: its events carry this handle's id.
    pub fn handle(&self) -> &IoHandle {
        &self.handle
    }

    /// The child's process id.
    pub fn pid(&self) -> Option<u32> {
        self.handle.pid()
    }

    /// Queues bytes for the program, as if typed.
    pub fn write(&self, bytes: Vec<u8>) -> io::Result<()> {
        if self.closed {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the terminal is closed",
            ));
        }
        self.control.write(&self.handle, bytes)
    }

    /// Tells the terminal, and so the program, its new size.
    pub fn resize(&self, size: PtySize) -> io::Result<()> {
        tcsetwinsize(&self.master, size.winsize())?;
        Ok(())
    }

    /// Signals the program, while it has not been reaped.
    pub fn signal(&self, signal: i32) {
        self.control.signal(&self.handle, signal);
    }

    /// Hands back what the consumer has finished with; see
    /// [`ReactorControl::credit`].
    pub fn credit(&self, weight: usize) {
        self.control.credit(&self.handle, weight);
    }

    /// Hangs up: the program gets `SIGHUP`, as it would when a terminal
    /// window is closed, and nothing more is heard from it. It is still
    /// reaped when it goes.
    pub fn close(&mut self) {
        if !self.closed {
            self.closed = true;
            self.control.signal(&self.handle, libc::SIGHUP);
            self.control.close(&self.handle);
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        self.close();
    }
}

/// The slave side of a freshly opened master.
pub(crate) fn open_slave(master: &OwnedFd) -> io::Result<OwnedFd> {
    let flags = OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC;
    // Straight from the master (Linux 4.13 and later), which cannot open the
    // wrong device whatever happens in /dev/pts meanwhile.
    if let Ok(slave) = rustix::pty::ioctl_tiocgptpeer(master, flags) {
        return Ok(slave);
    }
    let name = rustix::pty::ptsname(master.as_fd(), Vec::new())?;
    let slave = rustix::fs::open(
        name.as_c_str(),
        rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NOCTTY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    Ok(slave)
}
