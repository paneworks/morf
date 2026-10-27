//! The environment variables morf reads, through one parser: each is a
//! comma list of words (`MORF_LOG=frame,gpu`) and `key=value` items
//! (`MORF_PROFILE=slow=16`).
//!
//! - `MORF_LOG`: `a11y`, `audio`, `damage`, `density`, `frame`, `gpu`,
//!   `gpu_wait`, `jit`, `palette`, `present`, `wake`.
//! - `MORF_PROFILE`: `lua`, `gpu`, `gpu_commands`, `slow=<ms>`,
//!   `frame_split=<ms>`, `render_split=<ms>`.
//! - `MORF_PRESENT`: `swapchain`, `mode=<m>`, `buffers=<n>`,
//!   `modifier=<m>`, `wait_ms=<ms>`.
//! - `MORF_DEBUG`: `no_dual_source`, `capture_readback`, `layout_full`,
//!   `gpu_unshared`, `popup_auto`, `no_a11y`, `no_crash_handler`,
//!   `crash_core`.
//!
//! Paths, names and settings (`MORF_CONFIG`, `MORF_OUTPUT`, ...) are read
//! with [`var`].

use std::path::PathBuf;
use std::str::FromStr;

/// The items of one variable, in the order given.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Flags(Vec<(String, Option<String>)>);

impl Flags {
    /// Parses `text`: items split on commas, each trimmed, empty ones
    /// dropped; `key=value` splits at the first `=`.
    pub fn parse(text: &str) -> Self {
        Self(
            text.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(|item| match item.split_once('=') {
                    Some((key, value)) => (key.trim().to_owned(), Some(value.trim().to_owned())),
                    None => (item.to_owned(), None),
                })
                .collect(),
        )
    }

    /// Whether `item` is listed, as a word or as a key.
    pub fn has(&self, item: &str) -> bool {
        self.0.iter().any(|(key, _)| key == item)
    }

    /// The value given to `key`, the last one when it is listed twice.
    pub fn value(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .and_then(|(_, value)| value.as_deref())
    }

    /// The value given to `key`, parsed; `None` when absent or unparsable.
    pub fn parsed<T: FromStr>(&self, key: &str) -> Option<T> {
        self.value(key).and_then(|value| value.parse().ok())
    }

    /// Whether nothing is listed.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The items of the variable `name`; empty when it is unset.
pub fn flags(name: &str) -> Flags {
    var(name)
        .map(|text| Flags::parse(&text))
        .unwrap_or_default()
}

/// Whether the `MORF_LOG` channel `channel` is on.
pub fn log(channel: &str) -> bool {
    flags("MORF_LOG").has(channel)
}

/// Whether the `MORF_DEBUG` switch `switch` is on.
pub fn debug(switch: &str) -> bool {
    flags("MORF_DEBUG").has(switch)
}

/// The variable `name`'s text, when it is set and not empty.
pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|text| !text.is_empty())
}

/// The variable `name` as a path, when it is set and not empty.
pub fn path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|text| !text.is_empty())
        .map(PathBuf::from)
}

/// The variable `name` as a list of paths, split as `PATH` is.
pub fn paths(name: &str) -> Vec<PathBuf> {
    std::env::var_os(name)
        .map(|text| std::env::split_paths(&text).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::Flags;

    #[test]
    fn words_and_settings_read_in_one_list() {
        let flags = Flags::parse(" frame, gpu ,slow=16,,mode=fifo ,slow=20");
        assert!(flags.has("frame") && flags.has("gpu") && flags.has("slow"));
        assert!(!flags.has("wake"));
        assert_eq!(flags.value("mode"), Some("fifo"));
        assert_eq!(flags.parsed::<u64>("slow"), Some(20));
        assert_eq!(flags.parsed::<u64>("mode"), None);
        assert_eq!(flags.value("frame"), None);
        assert!(Flags::parse(" , ").is_empty());
    }
}
