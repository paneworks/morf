//! A runtime's children and connections, and the callbacks each one owes.
//!
//! The [`Reactor`] watches pipes and sockets on its own thread; the hub
//! keeps, for every handle it started, what kind of thing it is and the
//! callbacks (`C`, opaque here) its events go to. [`IoHub::collect`] turns
//! what the reactor sent into owed [`IoCall`]s, at most [`IO_BATCH`] per
//! handle per turn so a chatty child shares the loop, and the caller runs
//! them. A `run` gathers its output and answers once; a socket request
//! gathers its reply up to a cap. The script's side of a handle is an
//! [`IoLink`], which shares a [`HandleStatus`] with the hub's entry.

mod take;

use std::cell::Cell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::time::Duration;

use crate::{ConnectOptions, IoEvent, IoHandle, IoId, Reactor, ReactorControl, SpawnOptions};

/// Children one runtime may have running at once.
pub const MAX_PROCESSES: usize = 64;
/// Connections one runtime may have open at once.
pub const MAX_CONNECTIONS: usize = 64;
/// Callbacks one handle may have run per turn of the loop.
pub const IO_BATCH: usize = 64;
/// What a `run` keeps of its output unless told otherwise.
pub const RUN_DEFAULT_MAX_OUTPUT: usize = 8 * 1024 * 1024;
/// The most output a caller may ask a child to keep.
pub const MAX_OUTPUT_LIMIT: usize = 256 * 1024 * 1024;
/// A socket request's reply cap unless told otherwise.
pub const REQUEST_DEFAULT_MAX: usize = 8 * 1024 * 1024;
/// The largest reply cap a caller may ask for.
pub const REQUEST_MAX_LIMIT: usize = 64 * 1024 * 1024;
/// A socket request's deadline unless told otherwise.
pub const REQUEST_DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
/// The most one write to a child or a connection carries.
pub const MAX_WRITE: usize = 1024 * 1024;
/// The longest line a caller may ask for.
pub const MAX_LINE_LIMIT: usize = 16 * 1024 * 1024;

/// What a handle and its entry share.
#[derive(Debug, Default)]
pub struct HandleStatus {
    running: Cell<bool>,
    connected: Cell<bool>,
    /// Closed by the script: no callback runs for it again.
    closed: Cell<bool>,
}

impl HandleStatus {
    pub fn running(&self) -> bool {
        self.running.get()
    }

    pub fn connected(&self) -> bool {
        self.connected.get()
    }

    pub fn closed(&self) -> bool {
        self.closed.get()
    }
}

/// What a handle is, and who its events go to.
pub enum IoKind<C> {
    Spawn {
        on_stdout: Option<C>,
        on_stderr: Option<C>,
        on_exit: Option<C>,
    },
    Run {
        callback: Option<C>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    Connect {
        on_data: Option<C>,
        on_connect: Option<C>,
        on_close: Option<C>,
    },
    Request {
        callback: Option<C>,
        reply: Vec<u8>,
        max: usize,
    },
}

impl<C> IoKind<C> {
    /// A `run`, answered once with everything it wrote.
    pub fn run(callback: Option<C>) -> Self {
        Self::Run {
            callback,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    /// A socket request, answered once with a reply of at most `max` bytes.
    pub fn request(callback: Option<C>, max: usize) -> Self {
        Self::Request {
            callback,
            reply: Vec::new(),
            max,
        }
    }
}

struct Entry<C> {
    handle: IoHandle,
    status: Rc<HandleStatus>,
    kind: IoKind<C>,
    queue: VecDeque<IoEvent>,
    process: bool,
}

/// A callback owed, with what it is owed.
pub struct IoCall<C> {
    pub status: Rc<HandleStatus>,
    pub callback: C,
    pub args: CallArgs,
}

impl<C> IoCall<C> {
    /// Whether it should still run: its handle may have been closed since.
    pub fn live(&self) -> bool {
        !self.status.closed()
    }
}

/// What an owed callback is called with.
pub enum CallArgs {
    None,
    Bytes(Vec<u8>),
    Text(String),
    Exit {
        code: Option<i32>,
        signal: Option<i32>,
        timed_out: bool,
    },
    Run(RunResult),
    Reply(Result<Vec<u8>, String>),
}

/// How a `run` ended.
#[derive(Debug, Default)]
pub struct RunResult {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
    pub truncated: bool,
    pub error: Option<String>,
}

impl RunResult {
    /// Exited zero, in time, having started at all.
    pub fn ok(&self) -> bool {
        self.code == Some(0) && !self.timed_out && self.error.is_none()
    }
}

/// The runtime's processes and connections.
pub struct IoHub<C> {
    reactor: Option<Reactor>,
    entries: BTreeMap<IoId, Entry<C>>,
    /// Answers that need no reactor: a `run` whose program would not start.
    deferred: Vec<IoCall<C>>,
}

impl<C> Default for IoHub<C> {
    fn default() -> Self {
        Self {
            reactor: None,
            entries: BTreeMap::new(),
            deferred: Vec::new(),
        }
    }
}

impl<C> Drop for IoHub<C> {
    fn drop(&mut self) {
        // The reactor goes first, killing what it started, before the
        // callbacks it might have answered are released.
        self.reactor = None;
    }
}

/// A script's hold on a child or a connection.
pub struct IoLink {
    handle: IoHandle,
    control: ReactorControl,
    status: Rc<HandleStatus>,
}

impl IoLink {
    pub fn status(&self) -> &HandleStatus {
        &self.status
    }

    pub fn pid(&self) -> Option<u32> {
        self.handle.pid()
    }

    /// Writes `data`: refused past [`MAX_WRITE`] (the outer error), or
    /// failed because it is closed or the reactor said so (the inner one).
    pub fn write(&self, data: &[u8]) -> Result<Result<(), String>, String> {
        if data.len() > MAX_WRITE {
            return Err(format!("a write is at most {MAX_WRITE} bytes"));
        }
        if self.status.closed() {
            return Ok(Err("closed".into()));
        }
        Ok(self
            .control
            .write(&self.handle, data.to_vec())
            .map_err(|error| error.to_string()))
    }

    pub fn close_stdin(&self) {
        self.control.close_stdin(&self.handle);
    }

    /// Sends `signal` while the child runs; whether it was running.
    pub fn kill(&self, signal: i32) -> bool {
        let running = self.status.running();
        if running {
            self.control.signal(&self.handle, signal);
        }
        running
    }

    /// Closes the handle: no callback runs for it again.
    pub fn close(&self) {
        if !self.status.closed.replace(true) {
            self.control.close(&self.handle);
        }
        self.status.running.set(false);
        self.status.connected.set(false);
    }
}

impl<C> IoHub<C> {
    fn reactor(&mut self) -> Result<&Reactor, String> {
        if self.reactor.is_none() {
            self.reactor = Some(
                Reactor::new().map_err(|error| format!("cannot start the I/O reactor: {error}"))?,
            );
        }
        Ok(self.reactor.as_ref().expect("just made"))
    }

    fn count(&self, process: bool) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.process == process && !entry.status.closed())
            .count()
    }

    fn insert(
        &mut self,
        handle: IoHandle,
        status: Rc<HandleStatus>,
        kind: IoKind<C>,
        process: bool,
    ) {
        self.entries.insert(
            handle.id(),
            Entry {
                handle,
                status,
                kind,
                queue: VecDeque::new(),
                process,
            },
        );
    }

    /// Starts a child. The outer error is a refusal (too many running, no
    /// reactor); the inner one a program that would not start, which a
    /// `run` is also answered with through its callback.
    pub fn spawn(
        &mut self,
        options: SpawnOptions,
        kind: IoKind<C>,
    ) -> Result<Result<IoLink, String>, String> {
        if self.count(true) >= MAX_PROCESSES {
            return Err(format!("more than {MAX_PROCESSES} processes running"));
        }
        let program = options.command[0].clone();
        let handle = match self.reactor()?.spawn(options) {
            Ok(handle) => handle,
            Err(error) => {
                let message = format!("{program}: {error}");
                if let IoKind::Run {
                    callback: Some(callback),
                    ..
                } = kind
                {
                    self.deferred.push(IoCall {
                        status: Rc::new(HandleStatus::default()),
                        callback,
                        args: CallArgs::Run(RunResult {
                            error: Some(message.clone()),
                            ..RunResult::default()
                        }),
                    });
                    crate::wake_all();
                }
                return Ok(Err(message));
            }
        };
        let control = self.reactor()?.control();
        let status = Rc::new(HandleStatus::default());
        status.running.set(true);
        self.insert(handle.clone(), Rc::clone(&status), kind, true);
        Ok(Ok(IoLink {
            handle,
            control,
            status,
        }))
    }

    /// Opens a connection; refused past [`MAX_CONNECTIONS`].
    pub fn connect(&mut self, options: ConnectOptions, kind: IoKind<C>) -> Result<IoLink, String> {
        if self.count(false) >= MAX_CONNECTIONS {
            return Err(format!("more than {MAX_CONNECTIONS} connections open"));
        }
        let reactor = self.reactor()?;
        let handle = reactor.connect(options);
        let control = reactor.control();
        let status = Rc::new(HandleStatus::default());
        self.insert(handle.clone(), Rc::clone(&status), kind, false);
        Ok(IoLink {
            handle,
            control,
            status,
        })
    }
}

/// A process id a script may signal: above 1, so neither init nor a group.
pub fn signalable_pid(pid: i64) -> Result<i32, String> {
    i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 1)
        .ok_or_else(|| "kill takes a process id above 1".into())
}

#[cfg(test)]
mod tests;
