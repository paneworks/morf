//! Each completion rings the loop that asked for it and no other: two
//! stand-in loops, each on its own thread, and the rings each one counts.

use std::fs;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::*;

type Job = Box<dyn FnOnce() + Send>;

/// A thread with an alarm, standing in for one output's loop: work run on
/// it is asked for by that loop.
pub(crate) struct Loop {
    pub(crate) waker: Waker,
    jobs: Option<mpsc::Sender<Job>>,
    join: Option<JoinHandle<()>>,
}

impl Loop {
    pub(crate) fn start() -> Self {
        let (jobs, queue) = mpsc::channel::<Job>();
        let (ready, waker) = mpsc::channel();
        let join = thread::spawn(move || {
            let wake = Wake::new().unwrap();
            ready.send(crate::waker()).unwrap();
            for job in queue {
                job();
            }
            drop(wake);
        });
        Self {
            waker: waker.recv().unwrap(),
            jobs: Some(jobs),
            join: Some(join),
        }
    }

    /// Runs `job` on this loop's thread and hands back what it made.
    pub(crate) fn run<T: Send + 'static>(&self, job: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = mpsc::channel();
        let job: Job = Box::new(move || {
            let _ = tx.send(job());
        });
        self.jobs.as_ref().unwrap().send(job).unwrap();
        rx.recv().unwrap()
    }

    pub(crate) fn rings(&self) -> u64 {
        self.waker.rings()
    }

    /// Waits for at least `count` rings, then a moment more for any stray.
    pub(crate) fn settle(&self, count: u64) {
        let started = Instant::now();
        while self.rings() < count && started.elapsed() < Duration::from_secs(5) {
            thread::sleep(Duration::from_millis(5));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

impl Drop for Loop {
    fn drop(&mut self) {
        self.jobs.take();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("morf-wake-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_file_change_rings_only_the_loop_watching_it() {
    let dir = scratch("watch");
    let (mine, theirs) = (dir.join("mine"), dir.join("theirs"));
    let (asker, other) = (Loop::start(), Loop::start());
    let path = mine.clone();
    let watch = asker.run(move || Watch::new(&path, WatchOptions::default()).unwrap());
    let path = theirs.clone();
    let unrelated = other.run(move || Watch::new(&path, WatchOptions::default()).unwrap());
    fs::write(&mine, "x").unwrap();
    asker.settle(1);
    assert!(watch.has_pending());
    assert!(asker.rings() >= 1);
    assert_eq!(other.rings(), 0);
    fs::write(&theirs, "y").unwrap();
    other.settle(1);
    assert!(unrelated.has_pending());
    assert!(other.rings() >= 1);
    drop((watch, unrelated));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_signal_rings_only_the_loops_that_subscribed_to_it() {
    const NAME: &str = "org.morf.WakeRoutes";
    const PATH: &str = "/org/morf/WakeRoutes";
    const INTERFACE: &str = "org.morf.WakeRoutes";
    let Ok((service, _)) = DbusService::own(Bus::Session, NAME, PATH, true) else {
        return;
    };
    let (first, second, idle) = (Loop::start(), Loop::start(), Loop::start());
    let subscribe = |member: &'static str| {
        move || subscribe_signal(Bus::Session, NAME, PATH, INTERFACE, member).unwrap()
    };
    let rang = first.run(subscribe("Rang"));
    let both_first = first.run(subscribe("Both"));
    let both_second = second.run(subscribe("Both"));
    let _quiet = idle.run(subscribe("Never"));
    service
        .emit(PATH, INTERFACE, "Rang", &DbusValue::Integer(1))
        .unwrap();
    assert!(rang.next_value(Duration::from_secs(5)).is_some());
    first.settle(1);
    assert_eq!((first.rings(), second.rings(), idle.rings()), (1, 0, 0));
    service
        .emit(PATH, INTERFACE, "Both", &DbusValue::Integer(2))
        .unwrap();
    assert!(both_first.next_value(Duration::from_secs(5)).is_some());
    assert!(both_second.next_value(Duration::from_secs(5)).is_some());
    second.settle(1);
    assert_eq!((first.rings(), second.rings(), idle.rings()), (2, 1, 0));
}
