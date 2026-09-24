//! The reactor's thread: one `epoll` over every pipe, pidfd and socket.
//!
//! Level-triggered throughout. A readable fd is read a few times per wake and
//! then left for the next round, so one chatty child cannot keep the thread
//! from the others. A handle that is too far ahead of its consumer is taken
//! out of the set entirely (a pipe whose writer is gone reports a hang-up
//! whether asked or not, so merely asking for nothing would spin) and put
//! back when credited.

use std::collections::HashMap;
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::process::ExitStatusExt;
use std::process::Child;
use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use rustix::buffer::spare_capacity;
use rustix::event::{Timespec, epoll};
use rustix::io::Errno;
use rustix::net::{AddressFamily, SendFlags, SocketAddrUnix, SocketFlags, SocketType};

use crate::reactor::{
    CloseReason, ConnectOptions, Endpoint, HIGH_WATER, Inbox, IoEvent, IoId, LOW_WATER,
    LineSplitter, Order, OutputMode, Shared, SpawnOptions, StdinMode,
};

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

    // ------------------------------------------------------------ processes

    fn add_process(
        &mut self,
        id: IoId,
        shared: Arc<Shared>,
        mut child: Box<Child>,
        options: SpawnOptions,
    ) {
        let now = Instant::now();
        let splitter = || options.lines.then(|| LineSplitter::new(options.max_line));
        let stream = |fd: Option<OwnedFd>, slot| {
            let fd = nonblocking(fd?).ok()?;
            let registered =
                epoll::add(&self.epoll, &fd, token(id, slot), epoll::EventFlags::IN).is_ok();
            Some(Stream {
                fd,
                splitter: splitter(),
                registered,
            })
        };
        let stdout = stream(child.stdout.take().map(OwnedFd::from), STDOUT);
        let stderr = stream(child.stderr.take().map(OwnedFd::from), STDERR);
        debug_assert!(options.stdout != OutputMode::Capture || stdout.is_some());
        let stdin = child
            .stdin
            .take()
            .map(OwnedFd::from)
            .and_then(|fd| nonblocking(fd).ok());
        let pidfd = rustix::process::Pid::from_raw(child.id() as i32)
            .and_then(|pid| {
                rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty()).ok()
            })
            .filter(|fd| {
                epoll::add(&self.epoll, fd, token(id, PIDFD), epoll::EventFlags::IN).is_ok()
            });
        let reap_at = pidfd.is_none().then(|| now + REAP_POLL);
        let mut pending = Pending::default();
        let close_stdin_after = match options.stdin {
            StdinMode::Data(bytes) => {
                shared.outgoing.fetch_add(bytes.len(), Ordering::SeqCst);
                pending.push(bytes);
                true
            }
            _ => false,
        };
        self.procs.insert(
            id,
            Proc {
                shared,
                child,
                exit: None,
                pidfd,
                stdout,
                stderr,
                stdin,
                stdin_registered: false,
                pending,
                close_stdin_after,
                output_left: options.max_output,
                truncated: false,
                timeout_at: options.timeout.map(|timeout| now + timeout),
                kill_at: None,
                timed_out: false,
                grace_at: None,
                reap_at,
                paused: false,
                silent: false,
                detached: options.detached,
            },
        );
        self.flush_stdin(id);
    }

    fn flush_stdin(&mut self, id: IoId) {
        let Some(proc) = self.procs.get_mut(&id) else {
            return;
        };
        let Some(fd) = proc.stdin.as_ref() else {
            proc.pending.discard(&proc.shared);
            return;
        };
        match proc
            .pending
            .flush(&proc.shared, |bytes| rustix::io::write(fd, bytes))
        {
            Ok(true) => {
                if proc.stdin_registered {
                    unregister(&self.epoll, fd);
                    proc.stdin_registered = false;
                }
                if proc.close_stdin_after {
                    close_stdin(&self.epoll, proc);
                }
            }
            Ok(false) => {
                if !proc.stdin_registered {
                    proc.stdin_registered =
                        epoll::add(&self.epoll, fd, token(id, STDIN), epoll::EventFlags::OUT)
                            .is_ok();
                }
            }
            Err(_) => {
                // The child stopped reading; what it did not take is gone.
                proc.pending.discard(&proc.shared);
                close_stdin(&self.epoll, proc);
            }
        }
    }

    fn read_process(&mut self, id: IoId, slot: u64) {
        let Some(proc) = self.procs.get_mut(&id) else {
            return;
        };
        let stderr = slot == STDERR;
        for _ in 0..READS_PER_WAKE {
            let stream = if stderr {
                &mut proc.stderr
            } else {
                &mut proc.stdout
            };
            let Some(open) = stream.as_mut() else {
                break;
            };
            match rustix::io::read(&open.fd, &mut self.buffer[..]) {
                Ok(0) | Err(Errno::PIPE | Errno::IO | Errno::BADF) => {
                    let closed = stream.take().expect("open stream");
                    end_stream(&self.epoll, &mut self.out, id, proc, closed, stderr);
                    break;
                }
                Ok(read) => {
                    deliver(&mut self.out, id, proc, stderr, &self.buffer[..read]);
                    if over_high_water(&proc.shared) {
                        pause_process(&self.epoll, proc);
                        break;
                    }
                }
                Err(Errno::INTR) => {}
                Err(_) => break,
            }
        }
        self.finish_process(id);
    }

    fn reap(&mut self, id: IoId) {
        let Some(proc) = self.procs.get_mut(&id) else {
            return;
        };
        match proc.child.try_wait() {
            Ok(Some(status)) => {
                proc.exit = Some((status.code(), status.signal()));
            }
            Ok(None) => return,
            // Reaped by someone else; nothing more will be known.
            Err(_) => proc.exit = Some((None, None)),
        }
        if let Some(fd) = proc.pidfd.take() {
            unregister(&self.epoll, &fd);
        }
        proc.reap_at = None;
        proc.timeout_at = None;
        proc.kill_at = None;
        if proc.stdout.is_some() || proc.stderr.is_some() {
            proc.grace_at = Some(Instant::now() + EXIT_GRACE);
        }
        self.finish_process(id);
    }

    /// Reports and forgets a child that has exited and said everything.
    fn finish_process(&mut self, id: IoId) {
        let Some(proc) = self.procs.get(&id) else {
            return;
        };
        let Some((code, signal)) = proc.exit else {
            return;
        };
        if proc.stdout.is_some() || proc.stderr.is_some() {
            return;
        }
        let mut proc = self.procs.remove(&id).expect("present");
        close_stdin(&self.epoll, &mut proc);
        if !proc.silent {
            self.out.emit(
                &proc.shared,
                IoEvent::Exit {
                    id,
                    code,
                    signal,
                    timed_out: proc.timed_out,
                    truncated: proc.truncated,
                },
            );
        }
    }

    // ---------------------------------------------------------------- sockets

    fn add_socket(
        &mut self,
        id: IoId,
        shared: Arc<Shared>,
        mut options: Box<ConnectOptions>,
        address: Option<Result<std::net::SocketAddr, String>>,
    ) {
        let now = Instant::now();
        let mut pending = Pending::default();
        let greeting = std::mem::take(&mut options.greeting);
        shared.outgoing.fetch_add(greeting.len(), Ordering::SeqCst);
        pending.push(greeting);
        let sock = Sock {
            shared,
            splitter: options.lines.then(|| LineSplitter::new(options.max_line)),
            connect_by: now + options.connect_timeout,
            deadline: options.deadline.map(|deadline| now + deadline),
            options,
            fd: None,
            state: SockState::Resolving,
            registered: epoll::EventFlags::empty(),
            pending,
            retry_at: None,
            address,
            paused: false,
        };
        self.socks.insert(id, sock);
        self.start_connect(id);
    }

    fn start_connect(&mut self, id: IoId) {
        let Some(sock) = self.socks.get_mut(&id) else {
            return;
        };
        sock.retry_at = None;
        let attempt = match &sock.options.endpoint {
            Endpoint::Unix(path) => rustix::net::socket_with(
                AddressFamily::UNIX,
                SocketType::STREAM,
                SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
                None,
            )
            .and_then(|fd| {
                let address = SocketAddrUnix::new(path.as_path())?;
                let result = rustix::net::connect(&fd, &address);
                Ok((fd, result))
            }),
            Endpoint::Tcp { .. } => match &sock.address {
                None => return,
                Some(Err(message)) => {
                    let reason = CloseReason::Error(message.clone());
                    self.close_socket(id, reason);
                    return;
                }
                Some(Ok(address)) => rustix::net::socket_with(
                    if address.is_ipv4() {
                        AddressFamily::INET
                    } else {
                        AddressFamily::INET6
                    },
                    SocketType::STREAM,
                    SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
                    None,
                )
                .map(|fd| {
                    let result = rustix::net::connect(&fd, address);
                    (fd, result)
                }),
            },
        };
        let unix = matches!(sock.options.endpoint, Endpoint::Unix(_));
        match attempt {
            Err(error) => self.close_socket(id, connect_error(error)),
            Ok((fd, Ok(()))) => {
                sock.fd = Some(fd);
                self.connected(id);
            }
            Ok((fd, Err(Errno::INPROGRESS))) => {
                sock.fd = Some(fd);
                sock.state = SockState::Connecting;
                update_interest(&self.epoll, id, sock);
            }
            Ok((_, Err(Errno::AGAIN))) if unix => {
                sock.state = SockState::Connecting;
                sock.retry_at = Some(Instant::now() + CONNECT_RETRY);
            }
            Ok((_, Err(error))) => self.close_socket(id, connect_error(error)),
        }
    }

    fn connected(&mut self, id: IoId) {
        let Some(sock) = self.socks.get_mut(&id) else {
            return;
        };
        sock.state = SockState::Connected;
        self.out.emit(&sock.shared, IoEvent::Connected(id));
        self.flush_socket(id);
    }

    fn flush_socket(&mut self, id: IoId) {
        let Some(sock) = self.socks.get_mut(&id) else {
            return;
        };
        if !matches!(sock.state, SockState::Connected) {
            return;
        }
        let Some(fd) = sock.fd.as_ref() else {
            return;
        };
        let result = sock.pending.flush(&sock.shared, |bytes| {
            rustix::net::send(fd, bytes, SendFlags::NOSIGNAL)
        });
        match result {
            Ok(_) => update_interest(&self.epoll, id, sock),
            Err(error) => self.close_socket(id, CloseReason::Error(error.to_string())),
        }
    }

    fn socket_ready(&mut self, id: IoId, flags: epoll::EventFlags) {
        let Some(sock) = self.socks.get_mut(&id) else {
            return;
        };
        if matches!(sock.state, SockState::Connecting) {
            let Some(fd) = sock.fd.as_ref() else {
                return;
            };
            match rustix::net::sockopt::socket_error(fd) {
                Ok(Ok(())) => self.connected(id),
                Ok(Err(error)) | Err(error) => self.close_socket(id, connect_error(error)),
            }
            return;
        }
        if flags.contains(epoll::EventFlags::OUT) {
            self.flush_socket(id);
        }
        let Some(sock) = self.socks.get_mut(&id) else {
            return;
        };
        if sock.paused {
            if flags.intersects(epoll::EventFlags::HUP | epoll::EventFlags::ERR) {
                // Nothing will take these, and asking to write would only
                // report the hang-up again and again.
                sock.pending.discard(&sock.shared);
                update_interest(&self.epoll, id, sock);
            }
            return;
        }
        for _ in 0..READS_PER_WAKE {
            let Some(fd) = sock.fd.as_ref() else {
                return;
            };
            match rustix::io::read(fd, &mut self.buffer[..]) {
                Ok(0) => {
                    self.close_socket(id, CloseReason::Eof);
                    return;
                }
                Ok(read) => {
                    let data = &self.buffer[..read];
                    match sock.splitter.as_mut() {
                        Some(splitter) => {
                            let out = &mut self.out;
                            let shared = &sock.shared;
                            splitter.push(data, |line| out.emit(shared, IoEvent::Data(id, line)));
                        }
                        None => self
                            .out
                            .emit(&sock.shared, IoEvent::Data(id, data.to_vec())),
                    }
                    if over_high_water(&sock.shared) {
                        sock.paused = true;
                        update_interest(&self.epoll, id, sock);
                        return;
                    }
                }
                Err(Errno::INTR) => {}
                Err(Errno::AGAIN) => return,
                Err(error) => {
                    let reason = CloseReason::Error(io::Error::from(error).to_string());
                    self.close_socket(id, reason);
                    return;
                }
            }
        }
    }

    /// Reports and forgets a connection.
    fn close_socket(&mut self, id: IoId, reason: CloseReason) {
        let Some(mut sock) = self.socks.remove(&id) else {
            return;
        };
        if let Some(line) = sock.splitter.as_mut().and_then(LineSplitter::finish) {
            self.out.emit(&sock.shared, IoEvent::Data(id, line));
        }
        sock.pending.discard(&sock.shared);
        if let Some(fd) = sock.fd.take() {
            unregister(&self.epoll, &fd);
        }
        self.out.emit(&sock.shared, IoEvent::Closed { id, reason });
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

fn connect_error(error: Errno) -> CloseReason {
    CloseReason::Error(io::Error::from(error).to_string())
}

fn close_stdin(epoll: &OwnedFd, proc: &mut Proc) {
    if let Some(fd) = proc.stdin.take() {
        if proc.stdin_registered {
            unregister(epoll, &fd);
        }
        proc.stdin_registered = false;
    }
    proc.pending.discard(&proc.shared);
}

fn pause_process(epoll: &OwnedFd, proc: &mut Proc) {
    proc.paused = true;
    for stream in [proc.stdout.as_mut(), proc.stderr.as_mut()]
        .into_iter()
        .flatten()
    {
        if stream.registered {
            unregister(epoll, &stream.fd);
            stream.registered = false;
        }
    }
}

/// Output as the child's options want it: counted against `max_output`,
/// then split into lines or passed on whole.
fn deliver(out: &mut Outbox, id: IoId, proc: &mut Proc, stderr: bool, data: &[u8]) {
    if proc.silent {
        return;
    }
    let data = match proc.output_left.as_mut() {
        Some(left) => {
            let take = data.len().min(*left);
            *left -= take;
            if take < data.len() {
                proc.truncated = true;
            }
            &data[..take]
        }
        None => data,
    };
    if data.is_empty() {
        return;
    }
    let make = |bytes| {
        if stderr {
            IoEvent::Stderr(id, bytes)
        } else {
            IoEvent::Stdout(id, bytes)
        }
    };
    let stream = if stderr {
        proc.stderr.as_mut()
    } else {
        proc.stdout.as_mut()
    };
    match stream.and_then(|stream| stream.splitter.as_mut()) {
        Some(splitter) => {
            let shared = &proc.shared;
            splitter.push(data, |line| out.emit(shared, make(line)));
        }
        None => out.emit(&proc.shared, make(data.to_vec())),
    }
}

/// A stream at its end: its last partial line is delivered and it leaves
/// the set.
fn end_stream(
    epoll: &OwnedFd,
    out: &mut Outbox,
    id: IoId,
    proc: &mut Proc,
    mut stream: Stream,
    stderr: bool,
) {
    if stream.registered {
        unregister(epoll, &stream.fd);
    }
    if !proc.silent
        && let Some(line) = stream.splitter.as_mut().and_then(LineSplitter::finish)
    {
        let event = if stderr {
            IoEvent::Stderr(id, line)
        } else {
            IoEvent::Stdout(id, line)
        };
        out.emit(&proc.shared, event);
    }
}

/// Asks `epoll` for what the connection is waiting on now, and no more.
fn update_interest(epoll: &OwnedFd, id: IoId, sock: &mut Sock) {
    let Some(fd) = sock.fd.as_ref() else {
        return;
    };
    let mut wanted = epoll::EventFlags::empty();
    match sock.state {
        SockState::Resolving => {}
        SockState::Connecting => wanted |= epoll::EventFlags::OUT,
        SockState::Connected => {
            if !sock.paused {
                wanted |= epoll::EventFlags::IN;
            }
            if !sock.pending.is_empty() {
                wanted |= epoll::EventFlags::OUT;
            }
        }
    }
    if wanted == sock.registered {
        return;
    }
    let result = if wanted.is_empty() {
        epoll::delete(epoll, fd)
    } else if sock.registered.is_empty() {
        epoll::add(epoll, fd, token(id, SOCKET), wanted)
    } else {
        epoll::modify(epoll, fd, token(id, SOCKET), wanted)
    };
    if result.is_ok() {
        sock.registered = wanted;
    }
}
