//! The limits and the steps a script's file views take, kept beside the
//! views themselves.
//!
//! A binding reads the caller's values and calls these: how much a file
//! view moves at once ([`MAX_VIEW_BYTES`]), how large a document or stream
//! buffer may be ([`buffer_limit`]), and what opening, pointing elsewhere or
//! switching preloading on a [`FileDocument`] does, in order.

use std::io;

use crate::{FileDocument, FileEvent, FileView};

/// The most one read or write of a [`FileView`] moves.
pub const MAX_VIEW_BYTES: usize = 1024 * 1024;
/// A document's or stream collector's buffer unless the caller says.
pub const DEFAULT_BUFFER: usize = 1024 * 1024;
/// The largest buffer a caller may ask for.
pub const MAX_BUFFER: usize = 16 * 1024 * 1024;

/// A buffer size as a script gives it, 1..=[`MAX_BUFFER`]; `what` names
/// the call.
pub fn buffer_limit(maximum: i64, what: &str) -> Result<usize, String> {
    usize::try_from(maximum)
        .ok()
        .filter(|maximum| (1..=MAX_BUFFER).contains(maximum))
        .ok_or_else(|| format!("{what} maximum_bytes must be 1..{MAX_BUFFER}"))
}

impl FileEvent {
    /// `"changed"`, `"moved"` or `"deleted"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::Moved => "moved",
            Self::Deleted => "deleted",
        }
    }
}

impl FileView {
    /// Writes `bytes`, refused past [`MAX_VIEW_BYTES`].
    pub fn write_bounded(&self, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > MAX_VIEW_BYTES {
            return Err("file write exceeds 1 MiB".to_owned());
        }
        self.write(bytes).map_err(|error| error.to_string())
    }
}

/// How a [`FileDocument`] starts out.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocumentOptions {
    pub preload: bool,
    pub watch_changes: bool,
    pub atomic_writes: bool,
    pub maximum: usize,
}

impl Default for DocumentOptions {
    fn default() -> Self {
        Self {
            preload: true,
            watch_changes: false,
            atomic_writes: true,
            maximum: DEFAULT_BUFFER,
        }
    }
}

impl FileDocument {
    /// A document of `path`, loaded at once when it preloads.
    pub fn open(path: String, options: DocumentOptions) -> io::Result<Self> {
        let mut file = Self::new(path, options.maximum);
        file.set_preload(options.preload);
        file.set_atomic_writes(options.atomic_writes);
        if options.preload {
            file.reload();
        }
        if options.watch_changes {
            file.set_watch_changes(true)?;
        }
        Ok(file)
    }

    /// Points the document at `path` (empty for none), loading it when it
    /// preloads; whether it is loaded or need not be.
    pub fn retarget(&mut self, path: &str, preload: bool) -> io::Result<bool> {
        self.set_preload(preload);
        self.set_path(path)?;
        Ok(path.is_empty() || !preload || self.reload())
    }

    /// Turns preloading on or off, loading now when it turns on; whether it
    /// is loaded or need not be.
    pub fn switch_preload(&mut self, preload: bool) -> bool {
        self.set_preload(preload);
        !preload || self.loaded() || self.path().as_os_str().is_empty() || self.reload()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffers_are_bounded() {
        assert_eq!(buffer_limit(1, "x"), Ok(1));
        assert_eq!(
            buffer_limit(0, "file_view").unwrap_err(),
            "file_view maximum_bytes must be 1..16777216"
        );
        assert!(buffer_limit(MAX_BUFFER as i64 + 1, "x").is_err());
    }

    #[test]
    fn a_document_opens_retargets_and_preloads() {
        let dir = std::env::temp_dir().join(format!("morf-files-options-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.txt");
        std::fs::write(&path, b"hello").unwrap();
        let path = path.to_string_lossy().into_owned();

        let lazy = DocumentOptions {
            preload: false,
            ..DocumentOptions::default()
        };
        let mut file = FileDocument::open(path.clone(), lazy).unwrap();
        assert!(!file.loaded());
        assert!(file.switch_preload(true));
        assert_eq!(file.data(), Some(&b"hello"[..]));
        assert!(file.retarget("", true).unwrap());
        let missing = dir.join("missing").to_string_lossy().into_owned();
        assert!(!file.retarget(&missing, true).unwrap());
        assert!(file.retarget(&missing, false).unwrap());

        let view = FileView::new(&path);
        assert!(view.write_bounded(&vec![0; MAX_VIEW_BYTES + 1]).is_err());
        assert_eq!(FileEvent::Moved.name(), "moved");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
