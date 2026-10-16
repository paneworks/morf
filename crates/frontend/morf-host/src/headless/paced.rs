//! A headless run on the wall clock, paced as a compositor paces a shell:
//! what the frame accounting's stalled and unstalled numbers are taken on
//! where no compositor can be run.
//!
//! The refreshes come every `refresh` of wall time. A surface that asked
//! for a frame gets its callback at the first refresh after it asked,
//! stamped with that refresh, and until then the host holds its paints
//! back as it does for a compositor's callback. Between refreshes the host
//! turns as the shell's loop does: at once while it has work, otherwise
//! asleep until the next refresh or the next timer. The virtual clock is
//! kept on the wall clock, so timers fire when they would in a shell.

use std::time::{Duration, Instant};

use morf_app::{Backend, PRIMARY_LAYER, WindowId};

use super::Headless;
use crate::host::turn::Turn;

impl Headless {
    /// Runs `length` of wall time paced at `refresh`; the frame report
    /// is the host's to take after.
    pub fn run_paced(&mut self, length: Duration, refresh: Duration) {
        let Some(host) = self.host.as_mut() else {
            return self.advance(length, Duration::ZERO);
        };
        let Some(backend) = host.backend.as_headless_mut() else {
            return;
        };
        backend.set_paced(true);
        backend.request_frame(WindowId::Layer(PRIMARY_LAYER));
        let started = Instant::now();
        let mut clock = started;
        let mut stamped = started;
        let mut refreshed = started;
        while started.elapsed() < length {
            let now = Instant::now();
            self.runtime.advance_virtual_clock(now - clock);
            self.now += now - clock;
            clock = now;
            if now >= refreshed + refresh {
                let behind = (now - refreshed).as_nanos() / refresh.as_nanos().max(1);
                refreshed += refresh * behind as u32;
                if let Some(backend) = host.backend.as_headless_mut() {
                    backend.advance(refreshed - stamped);
                }
                stamped = refreshed;
            }
            let timer_due = self
                .runtime
                .next_virtual_deadline()
                .is_some_and(|due| due <= self.now);
            if host.settled() && !timer_due && !self.runtime.has_pending_work() {
                let next_timer = self
                    .runtime
                    .next_virtual_deadline()
                    .map(|due| now + due.saturating_sub(self.now));
                let wake = next_timer.map_or(refreshed + refresh, |at| at.min(refreshed + refresh));
                std::thread::sleep(wake.saturating_duration_since(Instant::now()));
                continue;
            }
            match host.turn(&mut self.runtime, None) {
                Ok(Turn::Again) => {}
                Ok(_) => break,
                Err(error) => {
                    if !self.problems.contains(&error) {
                        self.problems.push(error);
                    }
                    break;
                }
            }
        }
        if let Some(backend) = host.backend.as_headless_mut() {
            backend.set_paced(false);
        }
        self.last_frame = self.now;
        self.logs.extend(self.runtime.take_logs());
    }
}
