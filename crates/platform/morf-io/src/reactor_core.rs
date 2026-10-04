//! The reactor's thread: one `epoll` over every pipe, pidfd and socket.
//!
//! Level-triggered throughout. A readable fd is read a few times per wake and
//! then left for the next round, so one chatty child cannot keep the thread
//! from the others. A handle that is too far ahead of its consumer is taken
//! out of the set entirely (a pipe whose writer is gone reports a hang-up
//! whether asked or not, so merely asking for nothing would spin) and put
//! back when credited.

mod process;
mod socket;

use std::collections::HashMap;
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::process::Child;
use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use rustix::buffer::spare_capacity;
use rustix::event::{Timespec, epoll};
use rustix::io::Errno;

use crate::reactor::{
    CloseReason, ConnectOptions, HIGH_WATER, Inbox, IoEvent, IoId, LOW_WATER, LineSplitter, Order,
    Shared,
};

use process::{close_stdin, end_stream};
use socket::update_interest;

const BELL: u64 = 0;
const STDOUT: u64 = 0;
const STDERR: u64 = 1;
const STDIN: u64 = 2;
const PIDFD: u64 = 3;
const SOCKET: u64 = 4;

const READ_CHUNK: usize = 64 * 1024;
const READS_PER_WAKE: usize = 4;
/// How long a child's pipes may stay open after it has exited: a grandchild
/// holding them must not keep the exit from being reported.
const EXIT_GRACE: Duration = Duration::from_millis(250);
/// Between the polite signal a timeout sends and the one that is not.
const KILL_GRACE: Duration = Duration::from_secs(2);
/// Only when a pidfd could not be had.
const REAP_POLL: Duration = Duration::from_millis(50);
/// A Unix socket whose backlog is full refuses rather than waits.
const CONNECT_RETRY: Duration = Duration::from_millis(10);

fn token(id: IoId, slot: u64) -> epoll::EventData {
    epoll::EventData::new_u64((id << 3) | slot)
}

/// Where events go, and whether any went since the loop last rang.
struct Outbox {
    sender: mpsc::Sender<IoEvent>,
    sent: bool,
}

impl Outbox {
    fn emit(&mut self, shared: &Shared, event: IoEvent) {
        shared
            .undelivered
            .fetch_add(event.weight(), Ordering::SeqCst);
        let _ = self.sender.send(event);
        self.sent = true;
    }
}

/// Whether reads on this handle should stop until it is credited.
fn over_high_water(shared: &Shared) -> bool {
    if shared.undelivered.load(Ordering::SeqCst) < HIGH_WATER {
        return false;
    }
    shared.paused.store(true, Ordering::SeqCst);
    // Credited between the check and the flag: take it back, since the
    // consumer saw no flag and will not send a resume.
    !(shared.undelivered.load(Ordering::SeqCst) < LOW_WATER
        && shared.paused.swap(false, Ordering::SeqCst))
}

struct Stream {
    fd: OwnedFd,
    splitter: Option<LineSplitter>,
    registered: bool,
}

/// Bytes on their way into a pipe or a socket.
#[derive(Default)]
struct Pending {
    bytes: Vec<u8>,
    offset: usize,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.offset >= self.bytes.len()
    }

    fn push(&mut self, bytes: Vec<u8>) {
        if self.is_empty() {
            self.bytes = bytes;
            self.offset = 0;
        } else {
            if self.offset > self.bytes.len() / 2 {
                self.bytes.drain(..self.offset);
                self.offset = 0;
            }
            self.bytes.extend_from_slice(&bytes);
        }
    }

    /// Writes what it can. `Ok(true)` when everything is out, `Ok(false)`
    /// when the fd is full.
    fn flush(
        &mut self,
        shared: &Shared,
        mut write: impl FnMut(&[u8]) -> rustix::io::Result<usize>,
    ) -> io::Result<bool> {
        while !self.is_empty() {
            match write(&self.bytes[self.offset..]) {
                Ok(written) => {
                    self.offset += written;
                    shared.outgoing.fetch_sub(written, Ordering::SeqCst);
                }
                Err(Errno::INTR) => {}
                Err(Errno::AGAIN) => return Ok(false),
                Err(error) => return Err(error.into()),
            }
        }
        self.bytes = Vec::new();
        self.offset = 0;
        Ok(true)
    }

    fn discard(&mut self, shared: &Shared) {
        let left = self.bytes.len().saturating_sub(self.offset);
        shared.outgoing.fetch_sub(left, Ordering::SeqCst);
        self.bytes = Vec::new();
        self.offset = 0;
    }
}

struct Proc {
    shared: Arc<Shared>,
    child: Box<Child>,
    /// `(code, signal)` once reaped.
    exit: Option<(Option<i32>, Option<i32>)>,
    pidfd: Option<OwnedFd>,
    stdout: Option<Stream>,
    stderr: Option<Stream>,
    stdin: Option<OwnedFd>,
    stdin_registered: bool,
    pending: Pending,
    close_stdin_after: bool,
    output_left: Option<usize>,
    truncated: bool,
    timeout_at: Option<Instant>,
    kill_at: Option<Instant>,
    timed_out: bool,
    grace_at: Option<Instant>,
    reap_at: Option<Instant>,
    paused: bool,
    /// Closed by its owner: reaped, never reported.
    silent: bool,
    detached: bool,
}

enum SockState {
    Resolving,
    Connecting,
    Connected,
}

struct Sock {
    shared: Arc<Shared>,
    options: Box<ConnectOptions>,
    fd: Option<OwnedFd>,
    state: SockState,
    registered: epoll::EventFlags,
    splitter: Option<LineSplitter>,
    pending: Pending,
    connect_by: Instant,
    deadline: Option<Instant>,
    retry_at: Option<Instant>,
    address: Option<Result<std::net::SocketAddr, String>>,
    paused: bool,
}

pub(crate) struct Core {
    epoll: OwnedFd,
    inbox: Arc<Inbox>,
    out: Outbox,
    procs: HashMap<IoId, Proc>,
    socks: HashMap<IoId, Sock>,
    buffer: Vec<u8>,
}

fn unregister(epoll: &OwnedFd, fd: impl AsFd) {
    let _ = epoll::delete(epoll, fd);
}

fn nonblocking(fd: OwnedFd) -> io::Result<OwnedFd> {
    rustix::io::ioctl_fionbio(&fd, true)?;
    Ok(fd)
}

fn earliest(times: impl IntoIterator<Item = Option<Instant>>) -> Option<Instant> {
    times.into_iter().flatten().min()
}

impl Core {
    pub(crate) fn new(inbox: Arc<Inbox>, sender: mpsc::Sender<IoEvent>) -> io::Result<Self> {
        let epoll = epoll::create(epoll::CreateFlags::CLOEXEC)?;
        epoll::add(
            &epoll,
            &inbox.bell,
            epoll::EventData::new_u64(BELL),
            epoll::EventFlags::IN,
        )?;
        Ok(Self {
            epoll,
            inbox,
            out: Outbox {
                sender,
                sent: false,
            },
            procs: HashMap::new(),
            socks: HashMap::new(),
            buffer: vec![0; READ_CHUNK],
        })
    }

    pub(crate) fn run(mut self) {
        let mut ready = Vec::with_capacity(64);
        loop {
            let timeout = self.next_deadline().map(|at| {
                let wait = at.saturating_duration_since(Instant::now());
                Timespec {
                    tv_sec: wait.as_secs() as i64,
                    tv_nsec: i64::from(wait.subsec_nanos()),
                }
            });
            ready.clear();
            match epoll::wait(&self.epoll, spare_capacity(&mut ready), timeout.as_ref()) {
                Ok(_) | Err(Errno::INTR) => {}
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
            let mut bell = false;
            for event in ready.drain(..) {
                let data = event.data.u64();
                let flags = event.flags;
                if data == BELL {
                    bell = true;
                } else {
                    self.ready(data >> 3, data & 7, flags);
                }
            }
            if bell {
                let mut drain = [0u8; 8];
                let _ = rustix::io::read(&self.inbox.bell, &mut drain);
                let orders = std::mem::take(
                    &mut *self
                        .inbox
                        .orders
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()),
                );
                for order in orders {
                    if matches!(order, Order::Shutdown) {
                        self.shutdown();
                        return;
                    }
                    self.order(order);
                }
            }
            self.deadlines(Instant::now());
            if std::mem::take(&mut self.out.sent) {
                crate::wake_all();
            }
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        let procs = self
            .procs
            .values()
            .flat_map(|proc| [proc.timeout_at, proc.kill_at, proc.grace_at, proc.reap_at]);
        let socks = self.socks.values().flat_map(|sock| {
            let connecting = !matches!(sock.state, SockState::Connected);
            [
                connecting.then_some(sock.connect_by),
                sock.deadline,
                sock.retry_at,
            ]
        });
        earliest(procs.chain(socks))
    }

    fn shutdown(&mut self) {
        for (_, mut proc) in self.procs.drain() {
            proc.stdout = None;
            proc.stderr = None;
            proc.stdin = None;
            proc.pidfd = None;
            if proc.exit.is_some() {
                continue;
            }
            let mut child = proc.child;
            if proc.detached {
                let _ = thread::Builder::new()
                    .name("morf-io-reap".into())
                    .spawn(move || {
                        let _ = child.wait();
                    });
            } else {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        self.socks.clear();
    }

    fn order(&mut self, order: Order) {
        match order {
            Order::Process {
                id,
                shared,
                child,
                options,
            } => self.add_process(id, shared, child, *options),
            Order::Connect {
                id,
                shared,
                options,
                address,
            } => self.add_socket(id, shared, options, address),
            Order::Resolved { id, address } => {
                if let Some(sock) = self.socks.get_mut(&id) {
                    sock.address = Some(address);
                    self.start_connect(id);
                }
            }
            Order::Write(id, bytes) => self.write(id, bytes),
            Order::CloseStdin(id) => {
                if let Some(proc) = self.procs.get_mut(&id) {
                    proc.close_stdin_after = true;
                    if proc.pending.is_empty() {
                        close_stdin(&self.epoll, proc);
                    }
                }
            }
            Order::Signal(id, signal) => {
                if let Some(proc) = self.procs.get(&id)
                    && proc.exit.is_none()
                    && (1..=64).contains(&signal)
                {
                    // Not yet reaped, so the pid is still this child's.
                    unsafe {
                        libc::kill(proc.child.id() as i32, signal);
                    }
                }
            }
            Order::Close(id) => {
                if let Some(proc) = self.procs.get_mut(&id) {
                    proc.silent = true;
                    for stream in [proc.stdout.take(), proc.stderr.take()]
                        .into_iter()
                        .flatten()
                    {
                        unregister(&self.epoll, &stream.fd);
                    }
                    proc.pending.discard(&proc.shared);
                    close_stdin(&self.epoll, proc);
                    self.finish_process(id);
                }
                if let Some(mut sock) = self.socks.remove(&id) {
                    sock.pending.discard(&sock.shared);
                    if let Some(fd) = sock.fd.take() {
                        unregister(&self.epoll, &fd);
                    }
                }
            }
            Order::Resume(id) => {
                if let Some(proc) = self.procs.get_mut(&id) {
                    proc.paused = false;
                    for (slot, stream) in [(STDOUT, &mut proc.stdout), (STDERR, &mut proc.stderr)] {
                        if let Some(stream) = stream
                            && !stream.registered
                        {
                            stream.registered = epoll::add(
                                &self.epoll,
                                &stream.fd,
                                token(id, slot),
                                epoll::EventFlags::IN,
                            )
                            .is_ok();
                        }
                    }
                }
                if let Some(sock) = self.socks.get_mut(&id) {
                    sock.paused = false;
                    update_interest(&self.epoll, id, sock);
                }
            }
            Order::Shutdown => {}
        }
    }

    fn write(&mut self, id: IoId, bytes: Vec<u8>) {
        if let Some(proc) = self.procs.get_mut(&id) {
            if proc.stdin.is_none() {
                proc.shared
                    .outgoing
                    .fetch_sub(bytes.len(), Ordering::SeqCst);
                return;
            }
            proc.pending.push(bytes);
            self.flush_stdin(id);
        } else if let Some(sock) = self.socks.get_mut(&id) {
            sock.pending.push(bytes);
            self.flush_socket(id);
        }
    }

    // ----------------------------------------------------------- dispatching

    fn ready(&mut self, id: IoId, slot: u64, flags: epoll::EventFlags) {
        match slot {
            STDOUT | STDERR => self.read_process(id, slot),
            STDIN => self.flush_stdin(id),
            PIDFD => self.reap(id),
            SOCKET => self.socket_ready(id, flags),
            _ => {}
        }
    }

    fn deadlines(&mut self, now: Instant) {
        let due = |at: Option<Instant>| at.is_some_and(|at| at <= now);
        let ids = self.procs.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let Some(proc) = self.procs.get_mut(&id) else {
                continue;
            };
            if due(proc.reap_at) {
                proc.reap_at = Some(now + REAP_POLL);
                self.reap(id);
            }
            let Some(proc) = self.procs.get_mut(&id) else {
                continue;
            };
            if due(proc.timeout_at) {
                proc.timeout_at = None;
                proc.timed_out = true;
                proc.kill_at = Some(now + KILL_GRACE);
                unsafe {
                    libc::kill(proc.child.id() as i32, libc::SIGTERM);
                }
            }
            if due(proc.kill_at) {
                proc.kill_at = None;
                let _ = proc.child.kill();
            }
            if due(proc.grace_at) {
                if proc.paused {
                    proc.grace_at = Some(now + EXIT_GRACE);
                } else {
                    proc.grace_at = None;
                    for (stderr, stream) in
                        [(false, proc.stdout.take()), (true, proc.stderr.take())]
                    {
                        if let Some(stream) = stream {
                            end_stream(&self.epoll, &mut self.out, id, proc, stream, stderr);
                        }
                    }
                    self.finish_process(id);
                }
            }
        }
        let ids = self.socks.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let Some(sock) = self.socks.get(&id) else {
                continue;
            };
            let connecting = !matches!(sock.state, SockState::Connected);
            if due(sock.deadline) || (connecting && sock.connect_by <= now) {
                self.close_socket(id, CloseReason::TimedOut);
            } else if due(sock.retry_at) {
                self.start_connect(id);
            }
        }
    }
}
