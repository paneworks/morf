//! sysfs attributes, heard the way the kernel tells of them.
//!
//! inotify never fires on sysfs: an attribute is generated on each read,
//! not stored, so nothing "writes" it. What the kernel does instead is
//! `sysfs_notify`: a reader that has read the attribute once and then waits
//! in `poll(2)` for `POLLPRI` is woken when the attribute's value changes --
//! a backlight's `actual_brightness` when the brightness is set, a power
//! supply's `online`, a switch's `state`. Reading it again from the start
//! arms it for the next change.
//!
//! A [`crate::Watch`] on a file under `/sys` is that: the watcher thread
//! polls the attribute beside its inotify descriptor, and a wake is a
//! `Changed` for it, at once, with no timer anywhere.

use std::io;
use std::os::fd::OwnedFd;
use std::path::Path;

use rustix::fs::{Mode, OFlags};

/// The attribute at `path`, opened and armed, when `path` is a file under
/// `/sys`; `None` for anything else (watched with inotify as ever).
pub(crate) fn open_attribute(path: &Path) -> io::Result<Option<OwnedFd>> {
    if !path.starts_with("/sys") {
        return Ok(None);
    }
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(None);
    };
    if !meta.is_file() {
        return Ok(None);
    }
    let fd = rustix::fs::open(path, OFlags::RDONLY | OFlags::CLOEXEC, Mode::empty())?;
    rearm(&fd);
    Ok(Some(fd))
}

/// Reads the attribute from its start, which is what arms it for the next
/// change. What it says is not needed here: whoever watches reads it.
pub(crate) fn rearm(fd: &OwnedFd) {
    let mut buffer = [0u8; 4096];
    let _ = rustix::io::pread(fd, &mut buffer, 0);
}
