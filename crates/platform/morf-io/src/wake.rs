//! The loop's alarm: a way for a thread with news to end a poll early.
//!
//! Every service here runs on its own thread and reports through a channel,
//! and the loop that drains those channels sleeps in a `poll` on its Wayland
//! socket. Without this it woke only for the compositor or for its own
//! fallback timeout, so a timer, a line from a child, or a signal from the bus
//! waited up to that timeout to be seen — the whole shell felt a tenth of a
//! second behind. A `Wake` is an eventfd the loop polls alongside the socket;
//! a [`Waker`] taken where a request is made pokes the loop on that thread,
//! and `wake_all` pokes every one there is, for news nobody in particular
//! asked for. The poke is cheap and idempotent, and a loop that is busy
//! rather than waiting simply finds the fd readable next time round.

use rustix::event::{EventfdFlags, eventfd};
use rustix::fd::{AsFd, BorrowedFd, OwnedFd};
use rustix::io::{read, write};
use std::cell::RefCell;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

static WAKES: Mutex<Vec<(u64, Arc<OwnedFd>)>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// The alarm of the loop running on this thread, once it made a
    /// [`Wake`]: every `Wake` made here shares it, and a [`Waker`] taken
    /// here rings it.
    static ALARM: RefCell<Option<Arc<OwnedFd>>> = const { RefCell::new(None) };
}

/// One loop's alarm; registered for the life of the value.
pub struct Wake {
    fd: Arc<OwnedFd>,
    id: u64,
}

impl Wake {
    /// The alarm of the loop on this thread, made with the first one.
    pub fn new() -> io::Result<Self> {
        let fd = ALARM.with(|alarm| -> io::Result<_> {
            let mut alarm = alarm.borrow_mut();
            if let Some(fd) = alarm.as_ref() {
                return Ok(Arc::clone(fd));
            }
            let fd = Arc::new(eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)?);
            *alarm = Some(Arc::clone(&fd));
            Ok(fd)
        })?;
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        WAKES
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((id, Arc::clone(&fd)));
        Ok(Self { fd, id })
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
            .retain(|(id, _)| *id != self.id);
    }
}

/// Tells every waiting loop there is something to collect.
pub fn wake_all() {
    let wakes = WAKES.lock().unwrap_or_else(|error| error.into_inner());
    for (_, fd) in wakes.iter() {
        let _ = write(fd, &1u64.to_ne_bytes());
    }
}

/// Who to ring when something asked for now is ready: the loop on the
/// thread that took it, or every loop when no loop runs on that thread.
#[derive(Clone, Debug)]
pub struct Waker(Option<Weak<OwnedFd>>);

/// The [`Waker`] for news asked for on this thread.
pub fn waker() -> Waker {
    Waker(ALARM.with(|alarm| alarm.borrow().as_ref().map(Arc::downgrade)))
}

impl Waker {
    /// Rings its loop; a loop that has ended is left alone.
    pub fn wake(&self) {
        match &self.0 {
            Some(alarm) => {
                if let Some(fd) = alarm.upgrade() {
                    let _ = write(&*fd, &1u64.to_ne_bytes());
                }
            }
            None => wake_all(),
        }
    }
}

/// Rings one [`Waker`] when dropped, as [`WakeOnDrop`] rings every loop.
pub struct WakeOnDropOf(pub Waker);

impl Drop for WakeOnDropOf {
    fn drop(&mut self) {
        self.0.wake();
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
