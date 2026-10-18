//! A bounded server-processing acknowledgement before a connection is closed.
//! Unlike queue roundtrips this does not dispatch application/UI callbacks.
use super::WaylandError;
use rustix::{
    event::{PollFd, PollFlags, poll},
    time::Timespec,
};
use std::{
    os::fd::OwnedFd,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use wayland_backend::{
    client::{Backend, ObjectData, ObjectId},
    protocol::Message,
};
use wayland_client::{Connection, protocol::wl_display};

#[derive(Default)]
struct Acknowledged(AtomicBool);
impl ObjectData for Acknowledged {
    fn event(
        self: Arc<Self>,
        _: &Backend,
        _: Message<ObjectId, OwnedFd>,
    ) -> Option<Arc<dyn ObjectData>> {
        self.0.store(true, Ordering::Relaxed);
        None
    }
    fn destroyed(&self, _: ObjectId) {}
}

fn timed_out() -> WaylandError {
    WaylandError("Wayland request acknowledgement timed out".into())
}
fn failed(error: impl std::fmt::Display) -> WaylandError {
    WaylandError(format!("Wayland request acknowledgement failed: {error}"))
}

pub(super) fn confirm_requests(
    connection: &Connection,
    until: Instant,
) -> Result<(), WaylandError> {
    if Instant::now() >= until {
        return Err(timed_out());
    }
    let done = Arc::new(Acknowledged::default());
    connection
        .send_request(
            &connection.display(),
            wl_display::Request::Sync {},
            Some(done.clone()),
        )
        .map_err(failed)?;
    let backend = connection.backend();
    loop {
        backend.dispatch_inner_queue().map_err(failed)?;
        let remaining = until
            .checked_duration_since(Instant::now())
            .ok_or_else(timed_out)?;
        if done.0.load(Ordering::Relaxed) {
            return Ok(());
        }
        connection.flush().map_err(failed)?;
        let Some(guard) = connection.prepare_read() else {
            continue;
        };
        let timeout = Timespec {
            tv_sec: remaining.as_secs().min(i64::MAX as u64) as i64,
            tv_nsec: remaining.subsec_nanos() as i64,
        };
        let mut fds = [PollFd::from_borrowed_fd(backend.poll_fd(), PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(failed(error)),
            Ok(0) => return Err(timed_out()),
            Ok(_) => {
                guard.read().map_err(failed)?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        os::unix::net::UnixStream,
        sync::mpsc,
        thread,
        time::Duration,
    };

    fn sync_request(server: &mut UnixStream) -> u32 {
        server
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = [0; 12];
        server.read_exact(&mut request).unwrap();
        assert_eq!(u32::from_ne_bytes(request[..4].try_into().unwrap()), 1);
        assert_eq!(
            u32::from_ne_bytes(request[4..8].try_into().unwrap()),
            12 << 16
        );
        u32::from_ne_bytes(request[8..].try_into().unwrap())
    }
    fn acknowledge(server: &mut UnixStream, callback: u32) {
        for word in [callback, 12 << 16, 1] {
            server.write_all(&word.to_ne_bytes()).unwrap();
        }
    }

    #[test]
    fn socket_flush_is_not_a_server_processing_acknowledgement() {
        let (socket, mut server) = UnixStream::pair().unwrap();
        let connection = Connection::from_socket(socket).unwrap();
        let (send, receive) = mpsc::channel();
        let worker = thread::spawn(move || {
            send.send(confirm_requests(
                &connection,
                Instant::now() + Duration::from_secs(3),
            ))
            .unwrap()
        });
        let callback = sync_request(&mut server);
        assert!(matches!(receive.try_recv(), Err(mpsc::TryRecvError::Empty)));
        acknowledge(&mut server, callback);
        receive
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn a_silent_server_cannot_keep_unlock_waiting_forever() {
        let (socket, mut server) = UnixStream::pair().unwrap();
        let connection = Connection::from_socket(socket).unwrap();
        let error =
            confirm_requests(&connection, Instant::now() + Duration::from_millis(30)).unwrap_err();
        assert!(error.0.contains("timed out"));
        sync_request(&mut server);
    }

    #[test]
    fn disconnected_server_cannot_acknowledge_unlock() {
        let (socket, server) = UnixStream::pair().unwrap();
        let connection = Connection::from_socket(socket).unwrap();
        drop(server);
        assert!(confirm_requests(&connection, Instant::now() + Duration::from_secs(1)).is_err());
    }

    #[test]
    fn expired_deadline_sends_no_request() {
        let (socket, mut server) = UnixStream::pair().unwrap();
        let connection = Connection::from_socket(socket).unwrap();
        assert!(confirm_requests(&connection, Instant::now()).is_err());
        server.set_nonblocking(true).unwrap();
        assert_eq!(
            server.read(&mut [0]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}
