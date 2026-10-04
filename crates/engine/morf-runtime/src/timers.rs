//! One-shot and repeating timers, on the wall clock or a virtual one.
//!
//! A timer is a deadline. On the wall clock it is an instant the loop sleeps
//! until, which is how a timer costs nothing between firings -- no thread
//! ticking beside it, no wake that finds nothing due. A runtime told to keep
//! a virtual clock holds the deadline on that clock instead, and the timer
//! comes due when the clock is advanced past it: a test that waits a second
//! of a configuration's time takes no second of its own, and fires the same
//! handlers every run.

use std::collections::HashSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use morf_scene::NodeHandle;

use crate::Handler;

/// What makes a timer come due.
pub enum TimerSource {
    Wall { due: Instant },
    Virtual { due: Duration },
}

impl TimerSource {
    /// A timer every `interval`, on the virtual clock when `now` is one.
    pub fn every(interval: Duration, now: Option<Duration>) -> std::io::Result<Self> {
        if interval.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "timer interval cannot be zero",
            ));
        }
        Ok(match now {
            Some(now) => Self::Virtual {
                due: now + interval,
            },
            None => Self::Wall {
                due: Instant::now() + interval,
            },
        })
    }

    /// Whether the timer has come due since it was last asked, rescheduling
    /// it when it has. A timer that fell several intervals behind fires
    /// once, not once per interval missed.
    pub fn fire(&mut self, now: Option<Duration>, interval: Duration) -> bool {
        match self {
            Self::Wall { due } => reschedule(due, Instant::now(), interval),
            Self::Virtual { due } => now.is_some_and(|now| reschedule(due, now, interval)),
        }
    }

    /// When a virtual timer next comes due; nothing for a wall one.
    pub fn deadline(&self) -> Option<Duration> {
        match self {
            Self::Wall { .. } => None,
            Self::Virtual { due } => Some(*due),
        }
    }

    /// When a wall timer next comes due; nothing for a virtual one.
    pub fn wall_deadline(&self) -> Option<Instant> {
        match self {
            Self::Wall { due } => Some(*due),
            Self::Virtual { .. } => None,
        }
    }
}

/// Moves a deadline that `now` has reached on by one interval, or to one
/// interval from now if it fell further behind than that.
fn reschedule<T>(due: &mut T, now: T, interval: Duration) -> bool
where
    T: Copy + Ord + std::ops::Add<Duration, Output = T>,
{
    if now < *due {
        return false;
    }
    *due = *due + interval;
    if *due <= now {
        *due = now + interval;
    }
    true
}

/// One timer.
pub struct Timer {
    /// Names the timer to whoever holds its handle, so it can be cancelled.
    pub id: u64,
    pub source: TimerSource,
    pub handler: Handler,
    pub repeat: bool,
    pub interval: Duration,
    /// The `Timer` node it stands for, when it is one.
    pub node: Option<NodeHandle>,
    /// Where it was made (`file:line`, or the `Timer` node), for a log of
    /// wakes: an idle shell woken by a timer can say whose.
    pub origin: Rc<str>,
}

/// A timer that came due, to be fired once everything before it in the
/// turn has run.
pub struct DueTimer {
    pub id: u64,
    pub node: Option<NodeHandle>,
    pub repeat: bool,
    pub interval: Duration,
    pub handler: Handler,
    pub origin: Rc<str>,
}

/// Every timer, and the clock they run on.
#[derive(Default)]
pub struct Timers {
    timers: Vec<Timer>,
    /// One-shot timers that came due this turn and have not yet fired: a
    /// cancel before their turn in the batch takes them out, so it holds.
    due_one_shots: HashSet<u64>,
    /// The virtual clock's reading, when there is one.
    virtual_now: Option<Duration>,
    /// The last id handed out. Never reused: a handle to a timer that is
    /// gone must not cancel one made since.
    last_id: u64,
}

impl Timers {
    /// A fresh timer id.
    pub fn next_id(&mut self) -> u64 {
        self.last_id += 1;
        self.last_id
    }

    /// A deadline every `interval`, on whichever clock this keeps.
    pub fn source(&self, interval: Duration) -> std::io::Result<TimerSource> {
        TimerSource::every(interval, self.virtual_now)
    }

    /// How many timers are running.
    pub fn len(&self) -> usize {
        self.timers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.timers.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Timer> {
        self.timers.iter()
    }

    pub fn add(&mut self, timer: Timer) {
        self.timers.push(timer);
    }

    /// Stops a timer. Returns whether there was one to stop.
    pub fn cancel(&mut self, id: u64) -> bool {
        let before = self.timers.len();
        self.timers.retain(|timer| timer.id != id);
        self.due_one_shots.remove(&id);
        self.timers.len() != before
    }

    /// Whether a timer is still running.
    pub fn is_active(&self, id: u64) -> bool {
        self.timers.iter().any(|timer| timer.id == id)
    }

    /// The timer standing for `node`.
    pub fn for_node(&self, node: NodeHandle) -> Option<&Timer> {
        self.timers.iter().find(|timer| timer.node == Some(node))
    }

    /// Stops the timer standing for `node`. Returns whether there was one.
    pub fn remove_node(&mut self, node: NodeHandle) -> bool {
        match self
            .timers
            .iter()
            .position(|timer| timer.node == Some(node))
        {
            Some(index) => {
                self.timers.swap_remove(index);
                true
            }
            None => false,
        }
    }

    /// Keeps only the timers whose node `keep` says to (and every timer with
    /// no node).
    pub fn retain_nodes(&mut self, keep: impl Fn(NodeHandle) -> bool) {
        self.timers.retain(|timer| timer.node.is_none_or(&keep));
    }

    /// Every timer that came due, rescheduled; a one-shot is taken out and
    /// marked as due until [`Timers::settle`] is asked about it.
    pub fn collect_due(&mut self) -> Vec<DueTimer> {
        let now = self.virtual_now;
        let mut due = Vec::new();
        let mut index = 0;
        while index < self.timers.len() {
            let interval = self.timers[index].interval;
            if !self.timers[index].source.fire(now, interval) {
                index += 1;
                continue;
            }
            let timer = &self.timers[index];
            due.push(DueTimer {
                id: timer.id,
                node: timer.node,
                repeat: timer.repeat,
                interval,
                handler: timer.handler.clone(),
                origin: Rc::clone(&timer.origin),
            });
            if timer.repeat {
                index += 1;
            } else {
                self.due_one_shots.insert(timer.id);
                self.timers.swap_remove(index);
            }
        }
        due
    }

    /// Whether a timer collected as due should still fire, as far as the
    /// timers know: a repeating one must not have been cancelled since, a
    /// one-shot fires at most once. Takes a one-shot's mark either way.
    pub fn settle(&mut self, id: u64, repeat: bool) -> bool {
        let one_shot_pending = !repeat && self.due_one_shots.remove(&id);
        if repeat {
            self.is_active(id)
        } else {
            one_shot_pending
        }
    }

    /// Moves every timer made from now on to a virtual clock that stands at
    /// zero and moves only when [`Timers::advance`] says so.
    pub fn use_virtual_clock(&mut self) {
        if self.virtual_now.is_none() {
            self.virtual_now = Some(Duration::ZERO);
        }
    }

    /// The virtual clock's reading, or nothing when timers run off the wall.
    pub fn virtual_now(&self) -> Option<Duration> {
        self.virtual_now
    }

    /// Moves the virtual clock forward.
    pub fn advance(&mut self, by: Duration) {
        if let Some(now) = self.virtual_now.as_mut() {
            *now += by;
        }
    }

    /// When the earliest wall timer comes due.
    pub fn next_wall_deadline(&self) -> Option<Instant> {
        self.timers
            .iter()
            .filter_map(|timer| timer.source.wall_deadline())
            .min()
    }

    /// When the earliest virtual timer comes due.
    pub fn next_virtual_deadline(&self) -> Option<Duration> {
        self.timers
            .iter()
            .filter_map(|timer| timer.source.deadline())
            .min()
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::*;
    use crate::{HandlerId, HandlerRegistry};

    struct Nowhere;

    impl HandlerRegistry for Nowhere {
        fn release(&self, _: HandlerId) {}
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn timer(timers: &mut Timers, ms: u64, repeat: bool) -> u64 {
        let id = timers.next_id();
        let interval = Duration::from_millis(ms);
        let source = timers.source(interval).unwrap();
        timers.add(Timer {
            id,
            source,
            handler: Handler::new(HandlerId(id), Rc::new(Nowhere)),
            repeat,
            interval,
            node: None,
            origin: "test".into(),
        });
        id
    }

    #[test]
    fn a_virtual_timer_fires_only_when_the_clock_passes_it() {
        let mut timers = Timers::default();
        timers.use_virtual_clock();
        let every = timer(&mut timers, 100, true);
        let once = timer(&mut timers, 250, false);
        assert!(timers.collect_due().is_empty());
        timers.advance(Duration::from_millis(100));
        assert_eq!(
            timers
                .collect_due()
                .iter()
                .map(|due| due.id)
                .collect::<Vec<_>>(),
            [every]
        );
        assert_eq!(
            timers.next_virtual_deadline(),
            Some(Duration::from_millis(200))
        );
        timers.advance(Duration::from_millis(500));
        let due: Vec<u64> = timers.collect_due().iter().map(|due| due.id).collect();
        // Behind by several intervals: once, not once per interval missed.
        assert_eq!(due.len(), 2);
        assert!(timers.settle(once, false));
        assert!(!timers.settle(once, false), "a one-shot fires at most once");
        assert!(!timers.is_active(once));
        assert!(timers.settle(every, true));
    }

    #[test]
    fn a_cancel_between_collection_and_firing_holds() {
        let mut timers = Timers::default();
        timers.use_virtual_clock();
        let once = timer(&mut timers, 10, false);
        timers.advance(Duration::from_millis(10));
        assert_eq!(timers.collect_due().len(), 1);
        assert!(!timers.cancel(once), "already out of the list");
        assert!(!timers.settle(once, false));
    }
}
