//! Moving an offer's bytes through a pipe, off the loop.
//!
//! Every data transfer in Wayland is the same shape: one side hands the
//! compositor the write end of a pipe, the other side writes into it, and the
//! compositor never looks at the bytes. Reading on the loop would stall the
//! shell for as long as the source takes to answer — and a source that never
//! answers would stall it forever — so both directions run on a short-lived
//! thread with a deadline, and report back through a channel and a wake.

use rustix::event::{PollFd, PollFlags, poll};
use rustix::fd::{AsFd, OwnedFd};
use rustix::io::{Errno, read, write};
use rustix::pipe::{PipeFlags, pipe_with};
use rustix::time::Timespec;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::mime::MAX_OFFER_BYTES;

/// How long a source has to finish handing over what it offered.
pub const READ_DEADLINE: Duration = Duration::from_secs(10);

/// How many transfers may be in flight at once, each way.
pub const MAX_TRANSFERS: usize = 8;

/// One finished read, and `tag`: what it was for.
pub struct ReadDone<T> {
    pub tag: T,
    pub result: Result<Vec<u8>, String>,
}

/// A fresh close-on-exec pipe: `(read, write)`.
pub fn pipe() -> Result<(OwnedFd, OwnedFd), String> {
    pipe_with(PipeFlags::CLOEXEC).map_err(|error| format!("could not create a pipe: {error}"))
}

/// Takes a slot for one more transfer, or says there is none.
pub fn take_slot(active: &AtomicUsize) -> bool {
    active
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            (count < MAX_TRANSFERS).then_some(count + 1)
        })
        .is_ok()
}

/// Reads a pipe to its end on a thread, then reports and wakes the loop.
///
/// The caller has already taken a slot from `active`; this gives it back.
pub fn spawn_read<T: Send + 'static>(
    fd: OwnedFd,
    tag: T,
    tx: mpsc::Sender<ReadDone<T>>,
    waker: Option<fn()>,
    active: Arc<AtomicUsize>,
) {
    thread::spawn(move || {
        let result = read_all(&fd, MAX_OFFER_BYTES, READ_DEADLINE);
        let _ = tx.send(ReadDone { tag, result });
        active.fetch_sub(1, Ordering::Relaxed);
        if let Some(wake) = waker {
            wake();
        }
    });
}

/// Writes bytes into a pipe on a thread; the compositor closes nothing for us.
pub fn spawn_write(fd: OwnedFd, bytes: Arc<Vec<u8>>, active: Arc<AtomicUsize>) {
    thread::spawn(move || {
        let _ = write_all(&fd, &bytes, READ_DEADLINE);
        active.fetch_sub(1, Ordering::Relaxed);
    });
}

/// Puts a descriptor in non-blocking mode, so the deadline is the only wait.
///
/// A descriptor the compositor handed over arrives blocking, and a blocking
/// write to a pipe nobody drains would outlive any deadline.
fn nonblocking(fd: &OwnedFd) {
    if let Ok(flags) = rustix::fs::fcntl_getfl(fd) {
        let _ = rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK);
    }
}

fn remaining(deadline: Instant) -> Option<Timespec> {
    let left = deadline.checked_duration_since(Instant::now())?;
    Some(Timespec {
        tv_sec: left.as_secs().min(i64::MAX as u64) as i64,
        tv_nsec: i64::from(left.subsec_nanos()),
    })
}

/// Reads until end of file, the byte limit, or the deadline.
pub fn read_all(fd: &OwnedFd, limit: usize, within: Duration) -> Result<Vec<u8>, String> {
    nonblocking(fd);
    let deadline = Instant::now() + within;
    let mut bytes = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let Some(timeout) = remaining(deadline) else {
            return Err("the source took too long to answer".to_owned());
        };
        let mut fds = [PollFd::new(fd, PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Err("the source took too long to answer".to_owned()),
            Ok(_) => {}
            Err(Errno::INTR) => continue,
            Err(error) => return Err(format!("could not wait for the source: {error}")),
        }
        match read(fd, &mut chunk) {
            Ok(0) => return Ok(bytes),
            Ok(count) => {
                if bytes.len() + count > limit {
                    return Err(format!("the offer is larger than {limit} bytes"));
                }
                bytes.extend_from_slice(&chunk[..count]);
            }
            Err(Errno::INTR | Errno::AGAIN) => {}
            Err(error) => return Err(format!("could not read the offer: {error}")),
        }
    }
}

/// Writes every byte, or gives up at the deadline or a closed reader.
pub fn write_all(fd: &OwnedFd, mut bytes: &[u8], within: Duration) -> Result<(), String> {
    nonblocking(fd);
    let deadline = Instant::now() + within;
    while !bytes.is_empty() {
        let Some(timeout) = remaining(deadline) else {
            return Err("the reader took too long".to_owned());
        };
        let mut fds = [PollFd::new(fd, PollFlags::OUT)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Err("the reader took too long".to_owned()),
            Ok(_) => {}
            Err(Errno::INTR) => continue,
            Err(error) => return Err(error.to_string()),
        }
        match write(fd.as_fd(), bytes) {
            Ok(count) => bytes = &bytes[count..],
            Err(Errno::INTR | Errno::AGAIN) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pipe_round_trips_through_both_threads() {
        let (reader, writer) = pipe().unwrap();
        let payload = Arc::new((0..200_000u32).map(|n| n as u8).collect::<Vec<_>>());
        let active = Arc::new(AtomicUsize::new(1));
        spawn_write(writer, Arc::clone(&payload), Arc::clone(&active));
        let bytes = read_all(&reader, MAX_OFFER_BYTES, Duration::from_secs(5)).unwrap();
        assert_eq!(bytes, *payload);
    }

    #[test]
    fn a_read_stops_at_its_limit() {
        let (reader, writer) = pipe().unwrap();
        let active = Arc::new(AtomicUsize::new(1));
        spawn_write(writer, Arc::new(vec![7; 4096]), active);
        let error = read_all(&reader, 1000, Duration::from_secs(5)).unwrap_err();
        assert!(error.contains("larger"), "{error}");
    }

    #[test]
    fn a_silent_source_times_out() {
        let (reader, _writer) = pipe().unwrap();
        let error = read_all(&reader, 1000, Duration::from_millis(30)).unwrap_err();
        assert!(error.contains("too long"), "{error}");
    }

    #[test]
    fn slots_are_bounded() {
        let active = AtomicUsize::new(0);
        for _ in 0..MAX_TRANSFERS {
            assert!(take_slot(&active));
        }
        assert!(!take_slot(&active));
    }
}
