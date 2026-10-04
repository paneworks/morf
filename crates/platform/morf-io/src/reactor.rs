//! Child processes and sockets that report as things happen.
//!
//! The older `Process` and `Socket` are pulled: someone has to ask each one
//! whether it has anything, on a timer, with a timeout. A shell with a dozen
//! of them spent its idle time asking. Here one thread per [`Reactor`] waits
//! in `epoll` on every pipe, socket and pidfd it owns, reads what arrives,
//! and hands it over as [`IoEvent`]s on a channel, ringing
//! [`crate::wake_all`] once per batch so the main loop drains it at once
//! and otherwise sleeps.
//!
//! One thread rather than a reader per pipe: a child with stdout, stderr,
//! stdin and an exit to watch would be three or four threads, and a
//! configuration may run dozens. `epoll` also gives timeouts, connects and
//! writes that never block the loop for free, which threads would each have
//! to reinvent.
//!
//! Every queue is bounded. A handle whose undelivered output passes
//! [`HIGH_WATER`] stops being read until the consumer [credits]
//! (ReactorControl::credit) it back below the low mark, so a child that
//! prints faster than Lua can listen waits on its pipe instead of filling
//! memory. Writes are refused past [`MAX_OUTGOING`] queued bytes. A line is
//! at most `max_line` bytes; a longer one is cut there and the rest of it
//! dropped.
//!
//! Dropping the reactor kills and reaps every child it started, except those
//! spawned `detached`, which are left running and reaped by a thread of
//! their own when they exit.

mod lines;
mod options;

use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::os::fd::OwnedFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rustix::event::{EventfdFlags, eventfd};

pub use lines::LineSplitter;
pub use options::{ConnectOptions, Endpoint, OutputMode, SpawnOptions, StdinMode};

/// Undelivered bytes one handle may have before its reads stop.
pub const HIGH_WATER: usize = 1024 * 1024;
/// Where reading starts again.
pub const LOW_WATER: usize = 256 * 1024;
/// Bytes one handle may have waiting to be written.
pub const MAX_OUTGOING: usize = 4 * 1024 * 1024;
/// The longest line delivered whole unless asked otherwise.
pub const DEFAULT_MAX_LINE: usize = 64 * 1024;

/// Names one process or connection for the life of its reactor.
pub type IoId = u64;

/// Why a connection ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloseReason {
    /// The peer closed it.
    Eof,
    /// Its connect or its deadline ran out.
    TimedOut,
    Error(String),
}

impl CloseReason {
    pub fn describe(&self) -> String {
        match self {
            Self::Eof => "eof".to_owned(),
            Self::TimedOut => "timed out".to_owned(),
            Self::Error(message) => message.clone(),
        }
    }
}

/// Something that happened to one handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IoEvent {
    /// A line (without its newline) or a chunk from a child's stdout.
    Stdout(IoId, Vec<u8>),
    Stderr(IoId, Vec<u8>),
    /// The child is gone and all its output has been delivered. The last
    /// event for the handle.
    Exit {
        id: IoId,
        code: Option<i32>,
        signal: Option<i32>,
        timed_out: bool,
        truncated: bool,
    },
    Connected(IoId),
    /// A line or a chunk from a connection.
    Data(IoId, Vec<u8>),
    /// The connection is over. The last event for the handle.
    Closed {
        id: IoId,
        reason: CloseReason,
    },
}

impl IoEvent {
    pub fn id(&self) -> IoId {
        match self {
            Self::Stdout(id, _) | Self::Stderr(id, _) | Self::Connected(id) | Self::Data(id, _) => {
                *id
            }
            Self::Exit { id, .. } | Self::Closed { id, .. } => *id,
        }
    }

    /// What this event counts against its handle's [`HIGH_WATER`], and what
    /// the consumer credits back once it is handled. One more than the
    /// payload, so a flood of empty lines is still a flood.
    pub fn weight(&self) -> usize {
        match self {
            Self::Stdout(_, bytes) | Self::Stderr(_, bytes) | Self::Data(_, bytes) => {
                bytes.len() + 1
            }
            _ => 0,
        }
    }

    /// Whether nothing more will come for this handle.
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Exit { .. } | Self::Closed { .. })
    }
}

/// What the reactor thread and the handle's owner share.
#[derive(Default)]
pub(crate) struct Shared {
    pub(crate) undelivered: AtomicUsize,
    pub(crate) outgoing: AtomicUsize,
    pub(crate) paused: AtomicBool,
}

/// One process or connection, as its owner holds it.
#[derive(Clone)]
pub struct IoHandle {
    id: IoId,
    pid: Option<u32>,
    pub(crate) shared: Arc<Shared>,
}

impl IoHandle {
    pub fn id(&self) -> IoId {
        self.id
    }

    /// The child's process id; `None` for a connection.
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }
}

pub(crate) enum Order {
    Process {
        id: IoId,
        shared: Arc<Shared>,
        child: Box<Child>,
        options: Box<SpawnOptions>,
    },
    Connect {
        id: IoId,
        shared: Arc<Shared>,
        options: Box<ConnectOptions>,
        address: Option<Result<SocketAddr, String>>,
    },
    Resolved {
        id: IoId,
        address: Result<SocketAddr, String>,
    },
    Write(IoId, Vec<u8>),
    CloseStdin(IoId),
    Signal(IoId, i32),
    Close(IoId),
    Resume(IoId),
    Shutdown,
}

pub(crate) struct Inbox {
    pub(crate) orders: Mutex<Vec<Order>>,
    pub(crate) bell: OwnedFd,
}

/// The side of a reactor that can be handed around: writes, signals and
/// closes are orders to its thread. Orders after the reactor has gone are
/// dropped.
#[derive(Clone)]
pub struct ReactorControl {
    inbox: Arc<Inbox>,
}

impl ReactorControl {
    pub(crate) fn order(&self, order: Order) {
        self.inbox
            .orders
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(order);
        let _ = rustix::io::write(&self.inbox.bell, &1u64.to_ne_bytes());
    }

    /// Queues bytes for a child's stdin or a connection. Refused when the
    /// handle already has [`MAX_OUTGOING`] bytes waiting.
    pub fn write(&self, handle: &IoHandle, bytes: Vec<u8>) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let queued = handle.shared.outgoing.load(Ordering::SeqCst);
        if queued + bytes.len() > MAX_OUTGOING {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                format!("more than {MAX_OUTGOING} bytes waiting to be written"),
            ));
        }
        handle
            .shared
            .outgoing
            .fetch_add(bytes.len(), Ordering::SeqCst);
        self.order(Order::Write(handle.id, bytes));
        Ok(())
    }

    /// Ends a child's stdin once what is queued has been written.
    pub fn close_stdin(&self, handle: &IoHandle) {
        self.order(Order::CloseStdin(handle.id));
    }

    /// Signals a child that has not been reaped yet.
    pub fn signal(&self, handle: &IoHandle, signal: i32) {
        self.order(Order::Signal(handle.id, signal));
    }

    /// Drops a process's pipes or a connection. No further events come for
    /// it; a child is left to finish (and is reaped), not killed.
    pub fn close(&self, handle: &IoHandle) {
        self.order(Order::Close(handle.id));
    }

    /// Hands back what the consumer has finished with, restarting reads on
    /// a handle that was paused for being too far ahead.
    pub fn credit(&self, handle: &IoHandle, weight: usize) {
        if weight == 0 {
            return;
        }
        let before = handle
            .shared
            .undelivered
            .fetch_sub(weight, Ordering::SeqCst);
        if before.saturating_sub(weight) < LOW_WATER
            && handle.shared.paused.swap(false, Ordering::SeqCst)
        {
            self.order(Order::Resume(handle.id));
        }
    }
}

/// One thread watching processes and connections; see the module docs.
pub struct Reactor {
    control: ReactorControl,
    events: mpsc::Receiver<IoEvent>,
    next_id: AtomicU64,
    thread: Option<JoinHandle<()>>,
}

impl Reactor {
    pub fn new() -> io::Result<Self> {
        let bell = eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)?;
        let inbox = Arc::new(Inbox {
            orders: Mutex::new(Vec::new()),
            bell,
        });
        let (sender, events) = mpsc::channel();
        let core = crate::reactor_core::Core::new(Arc::clone(&inbox), sender)?;
        let thread = thread::Builder::new()
            .name("morf-io-reactor".into())
            .spawn(move || core.run())?;
        Ok(Self {
            control: ReactorControl { inbox },
            events,
            next_id: AtomicU64::new(1),
            thread: Some(thread),
        })
    }

    pub fn control(&self) -> ReactorControl {
        self.control.clone()
    }

    fn next_id(&self) -> IoId {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Starts a child. A child that cannot be started (no such program, no
    /// such directory) is an error here; everything after is an event.
    ///
    /// `LD_LIBRARY_PATH` is not passed on unless `environment` names it:
    /// morf may run under a wrapper that points it at libraries a system
    /// binary must not load.
    pub fn spawn(&self, options: SpawnOptions) -> io::Result<IoHandle> {
        let (program, args) = options.command.split_first().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "command cannot be empty")
        })?;
        let mut command = Command::new(program);
        command.args(args);
        if options.clear_environment {
            command.env_clear();
        } else {
            command.env_remove("LD_LIBRARY_PATH");
        }
        command.envs(&options.environment);
        if let Some(directory) = &options.working_directory {
            command.current_dir(directory);
        }
        if options.detached {
            command.process_group(0);
        }
        command.stdin(match options.stdin {
            StdinMode::Null => Stdio::null(),
            _ => Stdio::piped(),
        });
        let output = |mode| match mode {
            OutputMode::Capture => Stdio::piped(),
            OutputMode::Null => Stdio::null(),
            OutputMode::Inherit => Stdio::inherit(),
        };
        command.stdout(output(options.stdout));
        command.stderr(output(options.stderr));
        let child = command.spawn()?;
        Ok(self.adopt(child, options))
    }

    /// Watches a child someone else started, through whatever fds were left
    /// in its `stdout`, `stderr` and `stdin` slots.
    ///
    /// For a child that is not wired to plain pipes: a terminal hands in its
    /// pseudo-terminal's master side twice, once as the output to read and
    /// once as the input to write, and everything else — reads that stop
    /// when the consumer is behind, queued writes, the pidfd, the reap — is
    /// the same as for [`Reactor::spawn`]. `options` says only how the
    /// output is delivered (`lines`, `max_line`, `max_output`), `timeout`
    /// and `detached`; how the child was started is the caller's business.
    pub fn adopt(&self, child: Child, options: SpawnOptions) -> IoHandle {
        let id = self.next_id();
        let shared = Arc::new(Shared::default());
        let handle = IoHandle {
            id,
            pid: Some(child.id()),
            shared: Arc::clone(&shared),
        };
        self.control.order(Order::Process {
            id,
            shared,
            child: Box::new(child),
            options: Box::new(options),
        });
        handle
    }

    /// Opens a connection without waiting for it. Whether it worked comes
    /// as [`IoEvent::Connected`] or [`IoEvent::Closed`].
    pub fn connect(&self, options: ConnectOptions) -> IoHandle {
        let id = self.next_id();
        let shared = Arc::new(Shared::default());
        let handle = IoHandle {
            id,
            pid: None,
            shared: Arc::clone(&shared),
        };
        let address = match &options.endpoint {
            Endpoint::Unix(_) => None,
            Endpoint::Tcp { host, port } => match host.parse::<IpAddr>() {
                Ok(ip) => Some(Ok(SocketAddr::new(ip, *port))),
                Err(_) => {
                    // A name may take the resolver a while; that happens on a
                    // thread of its own and arrives as an order.
                    let control = self.control();
                    let (host, port) = (host.clone(), *port);
                    let resolved =
                        thread::Builder::new()
                            .name("morf-io-resolve".into())
                            .spawn(move || {
                                let address = (host.as_str(), port)
                                    .to_socket_addrs()
                                    .map_err(|error| error.to_string())
                                    .and_then(|mut found| {
                                        found.next().ok_or_else(|| format!("{host}: no address"))
                                    });
                                control.order(Order::Resolved { id, address });
                            });
                    match resolved {
                        Ok(_) => None,
                        Err(error) => Some(Err(error.to_string())),
                    }
                }
            },
        };
        self.control.order(Order::Connect {
            id,
            shared,
            options: Box::new(options),
            address,
        });
        handle
    }

    /// The next event, if one has arrived.
    pub fn try_next(&self) -> Option<IoEvent> {
        self.events.try_recv().ok()
    }

    /// The next event, waiting up to `timeout` for it.
    pub fn next_timeout(&self, timeout: Duration) -> Option<IoEvent> {
        self.events.recv_timeout(timeout).ok()
    }
}

impl Drop for Reactor {
    fn drop(&mut self) {
        self.control.order(Order::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Sends `signal` to process `pid`. Process groups and init (`pid <= 1`)
/// are refused.
pub fn signal_process(pid: i32, signal: i32) -> std::io::Result<()> {
    if pid <= 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "a process id above 1",
        ));
    }
    // SAFETY: kill(2) with a positive pid only sends a signal.
    if unsafe { libc::kill(pid, signal) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// A signal by its name (`"TERM"`, `"SIGKILL"`, any case) or its number.
pub fn signal_number(name: &str) -> Option<i32> {
    if let Ok(number) = name.parse::<i32>() {
        return (1..=64).contains(&number).then_some(number);
    }
    let upper = name.to_ascii_uppercase();
    let bare = upper.strip_prefix("SIG").unwrap_or(&upper);
    Some(match bare {
        "HUP" => libc::SIGHUP,
        "INT" => libc::SIGINT,
        "QUIT" => libc::SIGQUIT,
        "KILL" => libc::SIGKILL,
        "USR1" => libc::SIGUSR1,
        "USR2" => libc::SIGUSR2,
        "TERM" => libc::SIGTERM,
        "CONT" => libc::SIGCONT,
        "STOP" => libc::SIGSTOP,
        "TSTP" => libc::SIGTSTP,
        "WINCH" => libc::SIGWINCH,
        _ => return None,
    })
}
