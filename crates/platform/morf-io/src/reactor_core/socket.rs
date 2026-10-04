//! The reactor thread's sockets: connecting, retrying, reading, and writing.

use std::io;
use std::os::fd::OwnedFd;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use rustix::event::epoll;
use rustix::io::Errno;
use rustix::net::{AddressFamily, SendFlags, SocketAddrUnix, SocketFlags, SocketType};

use crate::reactor::{CloseReason, ConnectOptions, Endpoint, IoEvent, IoId, LineSplitter, Shared};

use super::{
    Core, CONNECT_RETRY, Pending, READS_PER_WAKE, SOCKET, Sock, SockState, over_high_water, token,
    unregister,
};

impl Core {
    // ---------------------------------------------------------------- sockets

    pub(super) fn add_socket(
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

    pub(super) fn start_connect(&mut self, id: IoId) {
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

    pub(super) fn flush_socket(&mut self, id: IoId) {
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

    pub(super) fn socket_ready(&mut self, id: IoId, flags: epoll::EventFlags) {
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
    pub(super) fn close_socket(&mut self, id: IoId, reason: CloseReason) {
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
}

fn connect_error(error: Errno) -> CloseReason {
    CloseReason::Error(io::Error::from(error).to_string())
}

/// Asks `epoll` for what the connection is waiting on now, and no more.
pub(super) fn update_interest(epoll: &OwnedFd, id: IoId, sock: &mut Sock) {
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
