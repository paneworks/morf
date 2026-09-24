//! The loop's alarm: a way for a thread with news to end a poll early.
//!
//! Every service here runs on its own thread and reports through a channel,
//! and the loop that drains those channels sleeps in a `poll` on its Wayland
//! socket. Without this it woke only for the compositor or for its own
//! fallback timeout, so a timer, a line from a child, or a signal from the bus
//! waited up to that timeout to be seen — the whole shell felt a tenth of a
//! second behind. A `Wake` is an eventfd the loop polls alongside the socket;
//! `wake_all` pokes every one there is, so a thread need not know which loop
//! is waiting for it. The poke is cheap and idempotent, and a loop that is
//! busy rather than waiting simply finds the fd readable next time round.

use rustix::event::{EventfdFlags, eventfd};
use rustix::fd::{AsFd, BorrowedFd, OwnedFd};
use rustix::io::{read, write};
use std::io;
use std::sync::{Arc, Mutex};

static WAKES: Mutex<Vec<Arc<OwnedFd>>> = Mutex::new(Vec::new());

/// One loop's alarm; registered for the life of the value.
pub struct Wake {
    fd: Arc<OwnedFd>,
}

impl Wake {
    pub fn new() -> io::Result<Self> {
        let fd = eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)?;
        let fd = Arc::new(fd);
        WAKES
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(Arc::clone(&fd));
        Ok(Self { fd })
    }

    /// Waits up to `timeout` for the alarm to ring, and says whether it
    /// did; it is left rung. For a loop with nothing else to wait on, and for
    /// tests that stand in for one.
    pub fn wait(&self, timeout: std::time::Duration) -> bool {
        use rustix::event::{PollFd, PollFlags, poll};
        let timeout = rustix::time::Timespec {
            tv_sec: timeout.as_secs().min(i64::MAX as u64) as i64,
            tv_nsec: timeout.subsec_nanos() as i64,
        };
        let mut fds = [PollFd::new(&self.fd, PollFlags::IN)];
        poll(&mut fds, Some(&timeout)).is_ok_and(|ready| ready > 0)
    }

    /// Clears the alarm; called after the poll returned because of it.
    pub fn drain(&self) {
        let mut buffer = [0u8; 8];
        let _ = read(&self.fd, &mut buffer);
    }
}

impl AsFd for Wake {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

impl Drop for Wake {
    fn drop(&mut self) {
        WAKES
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|other| !Arc::ptr_eq(other, &self.fd));
    }
}

/// Tells every waiting loop there is something to collect.
pub fn wake_all() {
    let wakes = WAKES.lock().unwrap_or_else(|error| error.into_inner());
    for fd in wakes.iter() {
        let _ = write(fd, &1u64.to_ne_bytes());
    }
}

/// Rings every loop when dropped: held by a worker thread for its whole
/// life, so the thread ending -- its channel hanging up, which a reader takes
/// as the end of the conversation -- is seen at once like any message.
pub struct WakeOnDrop;

impl Drop for WakeOnDrop {
    fn drop(&mut self) {
        wake_all();
    }
}
