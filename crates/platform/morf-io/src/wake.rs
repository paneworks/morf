//! The loop's alarm: a way for a thread with news to end a poll early.
//!
//! Every service here runs on its own thread and reports through a channel,
//! and the loop that drains those channels sleeps in a `poll` on its Wayland
//! socket. Without this it woke only for the compositor or for its own
//! fallback timeout, so a timer, a line from a child, or a signal from the bus
//! waited up to that timeout to be seen — the whole shell felt a tenth of a
//! second behind. A `Wake` is an eventfd the loop polls alongside the socket;
//! a [`Waker`] taken where a request is made pokes the loop on that thread,
//! and only that one: each output runs its own loop, and news one of them
//! asked for is nothing the others need to wake for. The poke is cheap and
//! idempotent, and a loop that is busy rather than waiting simply finds the
//! fd readable next time round.

use rustix::event::{EventfdFlags, eventfd};
use rustix::fd::{AsFd, BorrowedFd, OwnedFd};
use rustix::io::{read, write};
use std::cell::RefCell;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

/// One thread's eventfd, and how often it was rung.
#[derive(Debug)]
struct Alarm {
    fd: OwnedFd,
    rings: AtomicU64,
    thread: String,
}

impl Alarm {
    fn ring(&self) {
        self.rings.fetch_add(1, Ordering::Relaxed);
        let _ = write(&self.fd, &1u64.to_ne_bytes());
    }
}

static WAKES: Mutex<Vec<(u64, Arc<Alarm>)>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// The alarm of the loop running on this thread, made with the first
    /// [`Wake`] or [`Waker`] taken here and shared by every later one.
    static ALARM: RefCell<Option<Arc<Alarm>>> = const { RefCell::new(None) };
}

/// This thread's alarm, made on first use.
fn alarm() -> io::Result<Arc<Alarm>> {
    ALARM.with(|alarm| {
        let mut alarm = alarm.borrow_mut();
        if let Some(held) = alarm.as_ref() {
            return Ok(Arc::clone(held));
        }
        let made = Arc::new(Alarm {
            fd: eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)?,
            rings: AtomicU64::new(0),
            thread: std::thread::current()
                .name()
                .unwrap_or("unnamed")
                .to_owned(),
        });
        *alarm = Some(Arc::clone(&made));
        Ok(made)
    })
}

/// One loop's alarm; registered for the life of the value.
pub struct Wake {
    alarm: Arc<Alarm>,
    id: u64,
}

impl Wake {
    /// The alarm of the loop on this thread, made with the first one.
    pub fn new() -> io::Result<Self> {
        let alarm = alarm()?;
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        WAKES
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((id, Arc::clone(&alarm)));
        Ok(Self { alarm, id })
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
        let mut fds = [PollFd::new(&self.alarm.fd, PollFlags::IN)];
        poll(&mut fds, Some(&timeout)).is_ok_and(|ready| ready > 0)
    }

    /// Clears the alarm; called after the poll returned because of it.
    pub fn drain(&self) {
        let mut buffer = [0u8; 8];
        let _ = read(&self.alarm.fd, &mut buffer);
    }

    /// How many times this loop's alarm has been rung since it was made.
    pub fn rings(&self) -> u64 {
        self.alarm.rings.load(Ordering::Relaxed)
    }
}

impl AsFd for Wake {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.alarm.fd.as_fd()
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

/// Each live loop's thread name and how often its alarm was rung: what
/// shows whether news reaches only the loop that asked for it.
pub fn wake_counts() -> Vec<(String, u64)> {
    let wakes = WAKES.lock().unwrap_or_else(|error| error.into_inner());
    let mut seen: Vec<&Arc<Alarm>> = Vec::new();
    for (_, alarm) in wakes.iter() {
        if !seen.iter().any(|held| Arc::ptr_eq(held, alarm)) {
            seen.push(alarm);
        }
    }
    seen.into_iter()
        .map(|alarm| (alarm.thread.clone(), alarm.rings.load(Ordering::Relaxed)))
        .collect()
}

/// Who to ring when something asked for now is ready: the loop on the
/// thread that took it, and no other.
#[derive(Clone, Debug)]
pub struct Waker(Option<Weak<Alarm>>);

/// The [`Waker`] for news asked for on this thread. A thread with no
/// alarm yet gets one, which a [`Wake`] made there later shares.
pub fn waker() -> Waker {
    Waker(alarm().ok().as_ref().map(Arc::downgrade))
}

impl Waker {
    /// Rings its loop; a loop that has ended is left alone.
    pub fn wake(&self) {
        if let Some(alarm) = self.0.as_ref().and_then(Weak::upgrade) {
            alarm.ring();
        }
    }

    /// How many times its loop has been rung; 0 once the loop has ended.
    pub fn rings(&self) -> u64 {
        self.0
            .as_ref()
            .and_then(Weak::upgrade)
            .map_or(0, |alarm| alarm.rings.load(Ordering::Relaxed))
    }

    /// Whether both ring the same loop.
    pub fn same_loop(&self, other: &Waker) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => Weak::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// Rings one [`Waker`] when dropped: held by a worker thread for its whole
/// life, so the thread ending -- its channel hanging up, which a reader takes
/// as the end of the conversation -- is seen at once like any message.
pub struct WakeOnDropOf(pub Waker);

impl Drop for WakeOnDropOf {
    fn drop(&mut self) {
        self.0.wake();
    }
}
