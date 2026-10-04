//! Following the configuration's `.lua` files, so a change on disk becomes
//! one reload.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

/// How long the files must be quiet after a change before it is acted on:
/// a save, a `git checkout`, a formatter touching every file are one reload.
pub const RELOAD_SETTLE: Duration = Duration::from_millis(50);
/// The longest a stream of changes can put a reload off.
const RELOAD_SETTLE_MAX: Duration = Duration::from_secs(1);

/// Calls `changed` whenever a `.lua` file under `roots` is written, made,
/// moved or removed, while `enabled` says so; returns when `changed` says to
/// stop, or when the roots cannot be watched at all.
///
/// The roots are watched recursively through the shared inotify thread, so
/// this sleeps until something under them happens. Every event is only a
/// reason to look: once the files have been quiet for [`RELOAD_SETTLE`], the
/// `.lua` snapshot is taken again and compared, and only a difference counts
/// -- an editor's swap file, a settings file written beside the
/// configuration, a change made while watching was off, are not reloads.
pub fn follow_lua_files(
    roots: &[PathBuf],
    enabled: &AtomicBool,
    mut changed: impl FnMut(&BTreeMap<PathBuf, (u64, SystemTime)>) -> bool,
) {
    let watches = roots
        .iter()
        .filter_map(|root| {
            morf_io::Watch::new(root, morf_io::WatchOptions { recursive: true }).ok()
        })
        .collect::<Vec<_>>();
    if watches.is_empty() {
        return;
    }
    let mut snapshot = lua_snapshot(roots);
    loop {
        morf_io::wait_any(&watches, None);
        let started = std::time::Instant::now();
        loop {
            for watch in &watches {
                watch.drain();
            }
            if started.elapsed() >= RELOAD_SETTLE_MAX
                || !morf_io::wait_any(&watches, Some(RELOAD_SETTLE))
            {
                break;
            }
        }
        for watch in &watches {
            watch.drain();
        }
        let next = lua_snapshot(roots);
        if next == snapshot {
            continue;
        }
        snapshot = next;
        if enabled.load(Ordering::Acquire) && !changed(&snapshot) {
            return;
        }
    }
}

pub fn lua_snapshot(roots: &[PathBuf]) -> BTreeMap<PathBuf, (u64, SystemTime)> {
    let mut snapshot = BTreeMap::new();
    let mut pending = roots.to_vec();
    while let Some(path) = pending.pop() {
        let Ok(entries) = fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(entry.path());
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("lua") {
                continue;
            }
            if let Ok(metadata) = entry.metadata() {
                snapshot.insert(
                    path,
                    (
                        metadata.len(),
                        metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    ),
                );
            }
        }
    }
    snapshot
}
