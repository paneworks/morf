//! One bounded poll loop; no client can block the lock/UI thread.
use super::*;
use std::io::Write;

struct Connection {
    stream: UnixStream,
    identity: PeerIdentity,
    input: Vec<u8>,
    output: Vec<u8>,
    written: usize,
    expires: Instant,
}
pub(crate) fn serve(
    listener: UnixListener,
    wake: UnixStream,
    owner: u32,
    request: impl Fn(PeerIdentity, &[u8]) -> Option<Vec<u8>>,
) {
    serve_with_limit(listener, wake, owner, MAX_REQUEST, request)
}

pub(crate) fn serve_with_limit(
    listener: UnixListener,
    wake: UnixStream,
    owner: u32,
    limit: usize,
    request: impl Fn(PeerIdentity, &[u8]) -> Option<Vec<u8>>,
) {
    serve_with_work(listener, wake, owner, limit, request, || {});
}
/// Byte 1 stops the loop; byte 2 requests native background work. UI callers
/// use nonblocking writes and never perform persistence under their mutex.
pub(crate) fn serve_with_work(
    listener: UnixListener,
    wake: UnixStream,
    owner: u32,
    limit: usize,
    request: impl Fn(PeerIdentity, &[u8]) -> Option<Vec<u8>>,
    work: impl Fn(),
) {
    let mut wake = wake;
    assert!((1..=4096).contains(&limit));
    let mut peers: Vec<Connection> = Vec::new();
    loop {
        peers.retain(|peer| Instant::now() < peer.expires);
        let mut polls: Vec<_> = [listener.as_raw_fd(), wake.as_raw_fd()]
            .into_iter()
            .map(|fd| libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();
        polls.extend(peers.iter().map(|peer| libc::pollfd {
            fd: peer.stream.as_raw_fd(),
            events: if peer.output.is_empty() {
                libc::POLLIN
            } else {
                libc::POLLOUT
            },
            revents: 0,
        }));
        let timeout = peers
            .iter()
            .map(|peer| peer.expires.saturating_duration_since(Instant::now()))
            .min()
            .map_or(-1, |duration| {
                duration.as_millis().max(1).min(i32::MAX as u128) as i32
            });
        // SAFETY: initialized poll descriptors, all owners live through this call.
        let result =
            unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, timeout) };
        if result < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if polls[1].revents != 0 {
            let mut commands = [0u8; 64];
            match wake.read(&mut commands) {
                Ok(0) => break,
                Ok(count) => {
                    if commands[..count].contains(&1) {
                        break;
                    }
                    if commands[..count].contains(&2) {
                        work();
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => break,
            }
        }
        for index in (0..peers.len()).rev() {
            let readiness = polls[index + 2].revents;
            if readiness == 0 {
                continue;
            }
            let peer = &mut peers[index];
            let result = if !peer.output.is_empty() {
                peer.stream
                    .write(&peer.output[peer.written..])
                    .map(|count| {
                        peer.written += count;
                        if peer.written == peer.output.len() {
                            peer.output.clear();
                            peer.written = 0;
                        }
                        count != 0
                    })
            } else {
                let mut buffer = [0u8; 4096];
                peer.stream.read(&mut buffer).map(|count| {
                    if count == 0 || peer.input.len() + count > limit {
                        return false;
                    }
                    peer.input.extend_from_slice(&buffer[..count]);
                    if let Some(end) = peer.input.iter().position(|byte| *byte == b'\n') {
                        if end + 1 != peer.input.len() {
                            return false;
                        }
                        let Some(reply) = request(peer.identity, &peer.input[..end]) else {
                            return false;
                        };
                        peer.input.clear();
                        peer.output = reply;
                        peer.expires = Instant::now() + IDLE;
                    }
                    true
                })
            };
            if !matches!(result, Ok(true))
                && !matches!(&result,Err(error) if matches!(error.kind(),io::ErrorKind::WouldBlock|io::ErrorKind::Interrupted))
            {
                peers.swap_remove(index);
            }
        }
        if polls[0].revents & libc::POLLIN != 0
            && let Ok((stream, _)) = listener.accept()
            && peers.len() < MAX_PEERS
            && let Ok(identity) = peer(stream.as_fd())
            && (identity.uid == owner || identity.uid == 0)
            && stream.set_nonblocking(true).is_ok()
        {
            peers.push(Connection {
                stream,
                identity,
                input: Vec::new(),
                output: Vec::new(),
                written: 0,
                expires: Instant::now() + IDLE,
            });
        }
    }
}
