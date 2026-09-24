use std::fs;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{ChangeKind, FsChange, Wake, Watch, WatchOptions, watcher_threads};

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("morf-watch-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const WAIT: Duration = Duration::from_secs(3);

fn next(watch: &Watch) -> FsChange {
    watch
        .next_timeout(WAIT)
        .expect("a change within three seconds")
}

/// Everything that arrives until the watch has been quiet a while.
fn settle(watch: &Watch) -> Vec<FsChange> {
    let started = Instant::now();
    while !watch.has_pending() && started.elapsed() < Duration::from_secs(1) {
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(150));
    watch.drain()
}

fn kinds(changes: &[FsChange]) -> Vec<(PathBuf, ChangeKind)> {
    changes
        .iter()
        .map(|change| (change.path.clone(), change.kind))
        .collect()
}

#[test]
fn a_file_is_created_changed_and_deleted() {
    let dir = Scratch::new("cycle");
    let file = dir.join("settings.json");
    let watch = Watch::new(&file, WatchOptions::default()).unwrap();
    fs::write(&file, "one").unwrap();
    let change = next(&watch);
    assert_eq!(change.kind, ChangeKind::Created);
    assert_eq!(change.path, file);
    assert_eq!(change.name, Path::new("settings.json"));
    // The write that follows the creation is folded into it or comes on its
    // own as a change; either way nothing else is reported.
    let rest = settle(&watch);
    assert!(rest.iter().all(|change| change.kind == ChangeKind::Changed));

    fs::write(&file, "two").unwrap();
    assert_eq!(
        kinds(&settle(&watch)),
        vec![(file.clone(), ChangeKind::Changed)]
    );

    fs::remove_file(&file).unwrap();
    assert_eq!(
        kinds(&settle(&watch)),
        vec![(file.clone(), ChangeKind::Deleted)]
    );
}

#[test]
fn only_the_watched_name_is_reported() {
    let dir = Scratch::new("filter");
    let file = dir.join("wanted");
    fs::write(&file, "x").unwrap();
    let watch = Watch::new(&file, WatchOptions::default()).unwrap();
    fs::write(dir.join("other"), "y").unwrap();
    fs::write(&file, "z").unwrap();
    assert_eq!(kinds(&settle(&watch)), vec![(file, ChangeKind::Changed)]);
}

#[test]
fn a_rename_away_is_moved_and_one_onto_it_is_a_change() {
    let dir = Scratch::new("rename");
    let file = dir.join("history");
    fs::write(&file, "a").unwrap();
    let watch = Watch::new(&file, WatchOptions::default()).unwrap();
    fs::rename(&file, dir.join("elsewhere")).unwrap();
    assert_eq!(
        kinds(&settle(&watch)),
        vec![(file.clone(), ChangeKind::Moved)]
    );
    // Replaced by a rename while absent: it appears.
    fs::write(dir.join("tmp"), "b").unwrap();
    fs::rename(dir.join("tmp"), &file).unwrap();
    assert_eq!(
        kinds(&settle(&watch)),
        vec![(file.clone(), ChangeKind::Created)]
    );
    // Replaced by a rename while present: it changed.
    fs::write(dir.join("tmp"), "c").unwrap();
    fs::rename(dir.join("tmp"), &file).unwrap();
    assert_eq!(kinds(&settle(&watch)), vec![(file, ChangeKind::Changed)]);
}

#[test]
fn a_burst_is_one_change() {
    let dir = Scratch::new("burst");
    let file = dir.join("log");
    fs::write(&file, "").unwrap();
    let watch = Watch::new(&file, WatchOptions::default()).unwrap();
    for index in 0..200 {
        fs::write(&file, format!("{index}")).unwrap();
    }
    // An editor's save: the old file moved aside, a new one written.
    fs::rename(&file, dir.join("log~")).unwrap();
    fs::write(&file, "saved").unwrap();
    assert_eq!(kinds(&settle(&watch)), vec![(file, ChangeKind::Changed)]);
}

#[test]
fn a_file_made_and_removed_before_anyone_looked_is_nothing() {
    let dir = Scratch::new("transient");
    let watch = Watch::new(&dir.0, WatchOptions::default()).unwrap();
    let file = dir.join("lock");
    fs::write(&file, "x").unwrap();
    fs::remove_file(&file).unwrap();
    fs::write(dir.join("marker"), "y").unwrap();
    let changes = settle(&watch);
    assert_eq!(
        kinds(&changes),
        vec![(dir.join("marker"), ChangeKind::Created)]
    );
}

#[test]
fn a_missing_directory_is_followed_down_until_the_file_appears() {
    let dir = Scratch::new("deep");
    let file = dir.join("a/b/c/settings.json");
    let watch = Watch::new(&file, WatchOptions::default()).unwrap();
    fs::create_dir_all(dir.join("a/b/c")).unwrap();
    fs::write(&file, "{}").unwrap();
    let changes = settle(&watch);
    assert_eq!(changes[0].kind, ChangeKind::Created, "{changes:?}");
    assert_eq!(changes[0].path, file);
    fs::write(&file, "{ }").unwrap();
    assert_eq!(kinds(&settle(&watch)), vec![(file, ChangeKind::Changed)]);
}

#[test]
fn a_directory_reports_its_entries() {
    let dir = Scratch::new("entries");
    let watch = Watch::new(&dir.0, WatchOptions::default()).unwrap();
    fs::write(dir.join("one"), "1").unwrap();
    let change = next(&watch);
    assert_eq!(change.kind, ChangeKind::Created);
    assert_eq!(change.name, Path::new("one"));
    settle(&watch);
    fs::create_dir(dir.join("sub")).unwrap();
    fs::write(dir.join("sub/deeper"), "2").unwrap();
    // Not recursive: the new directory is reported, what goes into it is not.
    assert_eq!(
        kinds(&settle(&watch)),
        vec![(dir.join("sub"), ChangeKind::Created)]
    );
    fs::remove_file(dir.join("one")).unwrap();
    assert_eq!(
        kinds(&settle(&watch)),
        vec![(dir.join("one"), ChangeKind::Deleted)]
    );
}

#[test]
fn a_recursive_watch_sees_directories_made_later() {
    let dir = Scratch::new("recursive");
    fs::create_dir_all(dir.join("old/inner")).unwrap();
    let watch = Watch::new(&dir.0, WatchOptions { recursive: true }).unwrap();
    fs::write(dir.join("old/inner/a.lua"), "1").unwrap();
    let change = next(&watch);
    assert_eq!(change.path, dir.join("old/inner/a.lua"));
    assert_eq!(change.name, Path::new("old/inner/a.lua"));
    settle(&watch);
    fs::create_dir(dir.join("new")).unwrap();
    // Give the watcher a moment to add the new directory.
    settle(&watch);
    fs::write(dir.join("new/b.lua"), "2").unwrap();
    let changes = settle(&watch);
    assert!(
        changes
            .iter()
            .any(|change| change.path == dir.join("new/b.lua")),
        "{changes:?}"
    );
}

#[test]
fn a_watched_directory_going_away_is_reported_and_its_return_seen() {
    let dir = Scratch::new("vanish");
    let target = dir.join("plugins");
    fs::create_dir(&target).unwrap();
    let watch = Watch::new(&target, WatchOptions::default()).unwrap();
    fs::remove_dir(&target).unwrap();
    let changes = settle(&watch);
    assert!(
        changes
            .iter()
            .any(|change| change.path == target && change.kind == ChangeKind::Deleted),
        "{changes:?}"
    );
    fs::create_dir(&target).unwrap();
    settle(&watch);
    fs::write(target.join("x"), "1").unwrap();
    let changes = settle(&watch);
    assert!(
        changes.iter().any(|change| change.path == target.join("x")),
        "{changes:?}"
    );
}

#[test]
fn a_dropped_watch_hears_nothing_more_and_others_keep_working() {
    let dir = Scratch::new("drop");
    let file = dir.join("f");
    let kept = Watch::new(&file, WatchOptions::default()).unwrap();
    let dropped = Watch::new(&file, WatchOptions::default()).unwrap();
    drop(dropped);
    fs::write(&file, "x").unwrap();
    assert_eq!(next(&kept).kind, ChangeKind::Created);
}

#[test]
fn many_watches_share_one_thread() {
    let dir = Scratch::new("many");
    let watches = (0..200)
        .map(|index| {
            Watch::new(dir.join(&format!("file-{index}")), WatchOptions::default()).unwrap()
        })
        .collect::<Vec<_>>();
    // Other tests start and stop the shared thread as they run; while these
    // watches live there is one, and a moment's overlap at most while an old
    // one finishes exiting.
    let started = Instant::now();
    while watcher_threads() > 1 && started.elapsed() < WAIT {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(watcher_threads(), 1);
    for index in (0..200).step_by(37) {
        fs::write(dir.join(&format!("file-{index}")), "x").unwrap();
    }
    for index in (0..200).step_by(37) {
        let change = next(&watches[index]);
        assert_eq!(change.path, dir.join(&format!("file-{index}")));
    }
    assert!(!watches[1].has_pending());
}

#[test]
fn a_change_rings_the_loop() {
    let dir = Scratch::new("wake");
    let file = dir.join("f");
    let wake = Wake::new().unwrap();
    let watch = Watch::new(&file, WatchOptions::default()).unwrap();
    wake.drain();
    fs::write(&file, "x").unwrap();
    let fd = wake.as_fd();
    let mut fds = [rustix::event::PollFd::new(
        &fd,
        rustix::event::PollFlags::IN,
    )];
    // Other tests ring every loop too, so the question is only whether the
    // alarm is up once the change has arrived: nobody else drains it. The
    // watcher rings just after it files the change, hence a moment's grace.
    let started = Instant::now();
    while !watch.has_pending() && started.elapsed() < WAIT {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(watch.has_pending());
    let timeout = rustix::event::Timespec {
        tv_sec: 1,
        tv_nsec: 0,
    };
    assert_eq!(rustix::event::poll(&mut fds, Some(&timeout)).unwrap(), 1);
}

#[test]
fn the_old_pull_api_sits_on_the_shared_watcher() {
    let dir = Scratch::new("pull");
    let file = dir.join("doc");
    fs::write(&file, "a").unwrap();
    let watcher = crate::FileView::new(&file).watch().unwrap();
    fs::write(&file, "b").unwrap();
    assert_eq!(watcher.next_event(WAIT), Some(crate::FileEvent::Changed));
}
