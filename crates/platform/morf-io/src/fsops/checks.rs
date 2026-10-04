//! What a script hands the filesystem calls, checked and shaped before the
//! call runs.
//!
//! A binding reads the caller's values and passes them through these: byte
//! counts are bounded by [`MAX_BYTES`], a file mode must fit `0o7777`, a glob
//! pattern must have a sensible length. They also answer the small
//! questions about a path that need no I/O at all — its name, its parent,
//! its extension — and split a file's bytes into lines.

use std::path::Path;

use super::{Entry, EntryKind, MAX_DEPTH};

/// The most a single read or write moves, in bytes.
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// What a read returns without being told otherwise.
pub const DEFAULT_READ: u64 = 16 * 1024 * 1024;
/// The most files one asynchronous read reads.
pub const MAX_ASYNC_FILES: usize = 256;
/// The longest glob pattern, in bytes.
pub const MAX_PATTERN: usize = 4096;

/// A read's byte limit, 0..=[`MAX_BYTES`]; `what` names the call.
pub fn read_limit(limit: i64, what: &str) -> Result<u64, String> {
    u64::try_from(limit)
        .ok()
        .filter(|limit| *limit <= MAX_BYTES)
        .ok_or_else(|| format!("{what} limit must be 0..{MAX_BYTES}"))
}

/// A window `{ offset, length }` of a file; `length` defaults to
/// [`DEFAULT_READ`].
pub fn read_window(offset: Option<i64>, length: Option<i64>) -> Result<(u64, u64), String> {
    let offset = u64::try_from(offset.unwrap_or(0))
        .map_err(|_| "fs.read offset must not be negative".to_owned())?;
    let length = u64::try_from(length.unwrap_or(DEFAULT_READ as i64))
        .ok()
        .filter(|length| *length <= MAX_BYTES)
        .ok_or_else(|| format!("fs.read length must be 0..{MAX_BYTES}"))?;
    Ok((offset, length))
}

/// Refuses a write larger than [`MAX_BYTES`].
pub fn check_write(bytes: &[u8], what: &str) -> Result<(), String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(format!("{what} exceeds {MAX_BYTES} bytes"));
    }
    Ok(())
}

/// A permission mode, 0..=`0o7777`.
pub fn file_mode(mode: i64) -> Result<u32, String> {
    u32::try_from(mode)
        .ok()
        .filter(|mode| *mode <= 0o7777)
        .ok_or_else(|| "fs mode must be 0..0o7777".into())
}

/// A listing's depth, clamped to 0..=[`MAX_DEPTH`].
pub fn list_depth(depth: i64) -> usize {
    usize::try_from(depth.clamp(0, MAX_DEPTH as i64)).unwrap_or(0)
}

/// Refuses an empty glob pattern or one longer than [`MAX_PATTERN`].
pub fn check_pattern(pattern: &str) -> Result<(), String> {
    if pattern.is_empty() || pattern.len() > MAX_PATTERN {
        return Err(format!("fs.glob pattern must be 1..{MAX_PATTERN} bytes"));
    }
    Ok(())
}

/// The first `most` lines of `bytes`, without their `\n` or `\r\n`. A final
/// newline ends the last line; it does not start an empty one.
pub fn split_lines(bytes: &[u8], most: usize) -> Vec<&[u8]> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let body = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    body.split(|byte| *byte == b'\n')
        .take(most)
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .collect()
}

/// A question about what a path is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathTest {
    /// Anything at all, a dangling link included.
    Exists,
    Dir,
    File,
    Link,
}

/// Answers `test` about `path`, false when it cannot be asked.
pub fn path_is(path: &Path, test: PathTest) -> bool {
    match test {
        PathTest::Exists => std::fs::symlink_metadata(path).is_ok(),
        PathTest::Dir => path.is_dir(),
        PathTest::File => path.is_file(),
        PathTest::Link => path.is_symlink(),
    }
}

/// A piece of a path's text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathPart {
    Basename,
    /// The parent, `.` for a bare name.
    Dirname,
    Extension,
    Stem,
}

/// `part` of `path`, empty when it has none.
pub fn path_part(path: &Path, part: PathPart) -> String {
    let lossy = |text: &std::ffi::OsStr| text.to_string_lossy().into_owned();
    let answer = match part {
        PathPart::Basename => path.file_name().map(lossy),
        PathPart::Dirname => path.parent().map(|parent| {
            if parent.as_os_str().is_empty() {
                ".".to_owned()
            } else {
                parent.to_string_lossy().into_owned()
            }
        }),
        PathPart::Extension => path.extension().map(lossy),
        PathPart::Stem => path.file_stem().map(lossy),
    };
    answer.unwrap_or_default()
}

impl Entry {
    /// A folder, or a link to one.
    pub fn is_dir(&self) -> bool {
        self.kind == EntryKind::Dir || self.target_kind == Some(EntryKind::Dir)
    }

    /// A regular file, or a link to one.
    pub fn is_file(&self) -> bool {
        self.kind == EntryKind::File || self.target_kind == Some(EntryKind::File)
    }

    /// The extension, lowercased; empty when there is none.
    pub fn extension(&self) -> String {
        self.path
            .extension()
            .map(|extension| extension.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_and_modes_are_bounded() {
        assert_eq!(read_limit(10, "fs.read"), Ok(10));
        assert!(
            read_limit(-1, "fs.read")
                .unwrap_err()
                .starts_with("fs.read limit")
        );
        assert!(read_limit(MAX_BYTES as i64 + 1, "x").is_err());
        assert_eq!(read_window(None, None), Ok((0, DEFAULT_READ)));
        assert!(read_window(Some(-1), None).is_err());
        assert!(check_write(&[0; 4], "fs.write").is_ok());
        assert_eq!(file_mode(0o644), Ok(0o644));
        assert!(file_mode(0o10000).is_err());
        assert_eq!(list_depth(-3), 0);
        assert_eq!(list_depth(1_000), MAX_DEPTH);
        assert!(check_pattern("").is_err());
        assert!(check_pattern("*.rs").is_ok());
    }

    #[test]
    fn lines_drop_their_endings() {
        assert!(split_lines(b"", 10).is_empty());
        assert_eq!(split_lines(b"a\r\nb\n", 10), vec![&b"a"[..], b"b"]);
        assert_eq!(split_lines(b"a\n\nc", 2), vec![&b"a"[..], b""]);
    }

    #[test]
    fn path_parts() {
        let path = Path::new("dir/name.tar.gz");
        assert_eq!(path_part(path, PathPart::Basename), "name.tar.gz");
        assert_eq!(path_part(path, PathPart::Dirname), "dir");
        assert_eq!(path_part(path, PathPart::Extension), "gz");
        assert_eq!(path_part(path, PathPart::Stem), "name.tar");
        assert_eq!(path_part(Path::new("bare"), PathPart::Dirname), ".");
        assert_eq!(path_part(Path::new("/"), PathPart::Basename), "");
        assert!(path_is(Path::new("/"), PathTest::Dir));
        assert!(!path_is(Path::new("/no/such/path"), PathTest::Exists));
    }
}
