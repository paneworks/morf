//! Flow between the reactor thread and its consumer: events out with their
//! weight counted, reads paused past the high-water mark, and bytes queued
//! on their way into a pipe or a socket.

use std::io;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

use rustix::io::Errno;

use crate::reactor::{HIGH_WATER, IoEvent, LOW_WATER, Shared};

/// Where events go, and whether any went since the loop last rang.
pub(super) struct Outbox {
    pub(super) sender: mpsc::Sender<IoEvent>,
    pub(super) sent: bool,
}

impl Outbox {
    pub(super) fn emit(&mut self, shared: &Shared, event: IoEvent) {
        shared
            .undelivered
            .fetch_add(event.weight(), Ordering::SeqCst);
        let _ = self.sender.send(event);
        self.sent = true;
    }
}

/// Whether reads on this handle should stop until it is credited.
pub(super) fn over_high_water(shared: &Shared) -> bool {
    if shared.undelivered.load(Ordering::SeqCst) < HIGH_WATER {
        return false;
    }
    shared.paused.store(true, Ordering::SeqCst);
    // Credited between the check and the flag: take it back, since the
    // consumer saw no flag and will not send a resume.
    !(shared.undelivered.load(Ordering::SeqCst) < LOW_WATER
        && shared.paused.swap(false, Ordering::SeqCst))
}

/// Bytes on their way into a pipe or a socket.
#[derive(Default)]
pub(super) struct Pending {
    bytes: Vec<u8>,
    offset: usize,
}

impl Pending {
    pub(super) fn is_empty(&self) -> bool {
        self.offset >= self.bytes.len()
    }

    pub(super) fn push(&mut self, bytes: Vec<u8>) {
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
    pub(super) fn flush(
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

    pub(super) fn discard(&mut self, shared: &Shared) {
        let left = self.bytes.len().saturating_sub(self.offset);
        shared.outgoing.fetch_sub(left, Ordering::SeqCst);
        self.bytes = Vec::new();
        self.offset = 0;
    }
}
