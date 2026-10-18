//! Counts of the waits the present path makes on the CPU.
//!
//! Each of them holds an output's whole loop while it lasts, so how often
//! they happen is what tells a stall from a slow frame.

use std::sync::atomic::{AtomicU64, Ordering};

/// How often the present path has waited, since the process started.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PresentStalls {
    /// Frames that waited for the compositor to give a buffer back.
    pub ring_waits: u64,
    /// Frames skipped because no buffer came back in time.
    pub ring_skips: u64,
    /// Waits on a dmabuf fence made on the CPU because the kernel or driver
    /// would not take it as a GPU wait.
    pub fence_waits: u64,
    /// Those fence waits that ran out their time.
    pub fence_timeouts: u64,
    /// Captures dropped because acquiring them back did not finish in time.
    pub acquire_timeouts: u64,
}

pub(crate) static RING_WAITS: AtomicU64 = AtomicU64::new(0);
pub(crate) static RING_SKIPS: AtomicU64 = AtomicU64::new(0);
pub(crate) static FENCE_WAITS: AtomicU64 = AtomicU64::new(0);
pub(crate) static FENCE_TIMEOUTS: AtomicU64 = AtomicU64::new(0);
pub(crate) static ACQUIRE_TIMEOUTS: AtomicU64 = AtomicU64::new(0);

pub(crate) fn count(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

/// The waits counted so far.
pub fn present_stalls() -> PresentStalls {
    let read = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
    PresentStalls {
        ring_waits: read(&RING_WAITS),
        ring_skips: read(&RING_SKIPS),
        fence_waits: read(&FENCE_WAITS),
        fence_timeouts: read(&FENCE_TIMEOUTS),
        acquire_timeouts: read(&ACQUIRE_TIMEOUTS),
    }
}
