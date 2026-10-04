//! The shell's log: lines with a level and a time, capped so a shell that
//! warns every second does not hold a day of them.

/// The most log entries kept, and the longest one.
pub const MAX_LOG_ENTRIES: usize = 2000;
pub const MAX_LOG_MESSAGE: usize = 4096;

/// How much a log line matters.
///
/// Ordered, so a filter is a comparison: asking for warnings gets errors too.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LogLevel {
    /// Something a person tuning a configuration wants and nobody else does.
    Debug,
    /// What happened, when it is worth saying.
    Info,
    /// Something went wrong and the shell carried on.
    Warn,
    /// Something went wrong and did not.
    Error,
}

impl LogLevel {
    /// The name on the wire and on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    /// Reads a level back, for `--level` and for the wire.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" | "warning" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

/// One line of the shell's log.
///
/// Carries when and how much rather than only what. A flat list of strings is
/// unreadable by the time it matters: a shell that has been running for a day
/// has thousands of them and no way to ask which are serious, or recent.
#[derive(Clone, Debug)]
pub struct LogEntry {
    pub level: LogLevel,
    /// Milliseconds since the epoch. A number rather than a formatted time, so
    /// whoever shows it decides how.
    pub at_ms: u64,
    pub message: String,
}

impl std::fmt::Display for LogEntry {
    /// Level and message, which is what a person reading one wants.
    ///
    /// Not the timestamp: it is a number of milliseconds and whoever shows it
    /// decides how, which is the whole reason it is stored as one.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:<5} {}", self.level.name(), self.message)
    }
}

impl LogEntry {
    /// Packs an entry into one wire string.
    ///
    /// Unit separators, because they cannot occur in a log message and so need
    /// no escaping -- and a log format that needs escaping gets it wrong on the
    /// one line you most wanted to read.
    pub fn to_wire(&self) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}",
            self.level.name(),
            self.at_ms,
            self.message
        )
    }

    /// Reads one back, tolerating a line that was never packed.
    pub fn from_wire(line: &str) -> Self {
        let mut parts = line.splitn(3, '\u{1f}');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(level), Some(at), Some(message)) => Self {
                level: LogLevel::parse(level).unwrap_or(LogLevel::Info),
                at_ms: at.parse().unwrap_or(0),
                message: message.to_owned(),
            },
            // An unpacked line came from somewhere else, and losing it would be
            // worse than showing it without a level.
            _ => Self {
                level: LogLevel::Info,
                at_ms: 0,
                message: line.to_owned(),
            },
        }
    }
}

/// Every line since the last time they were taken, oldest first.
#[derive(Default)]
pub struct Log {
    entries: Vec<LogEntry>,
}

impl Log {
    /// Records one line, stamped with when it happened.
    ///
    /// The one way in, so every entry gets a level and a time rather than the
    /// flat strings this used to hold -- a shell running for a day accumulates
    /// thousands, and without either there is no way to ask which are serious
    /// or recent.
    pub fn push(&mut self, level: LogLevel, message: impl Into<String>) {
        let at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_millis() as u64)
            .unwrap_or(0);
        // A cap, oldest out first: a shell that logs a warning a second
        // would otherwise hold a day of them.
        if self.entries.len() >= MAX_LOG_ENTRIES {
            let excess = self.entries.len() + 1 - MAX_LOG_ENTRIES;
            self.entries.drain(..excess);
        }
        let mut message = message.into();
        if message.len() > MAX_LOG_MESSAGE {
            let mut cut = MAX_LOG_MESSAGE;
            while !message.is_char_boundary(cut) {
                cut -= 1;
            }
            message.truncate(cut);
            message.push('…');
        }
        self.entries.push(LogEntry {
            level,
            at_ms,
            message,
        });
    }

    /// Hands over every line, leaving none.
    pub fn take(&mut self) -> Vec<LogEntry> {
        std::mem::take(&mut self.entries)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_keeps_the_newest_lines_and_cuts_a_long_one() {
        let mut log = Log::default();
        for index in 0..MAX_LOG_ENTRIES + 5 {
            log.push(LogLevel::Info, format!("{index}"));
        }
        assert_eq!(log.len(), MAX_LOG_ENTRIES);
        assert_eq!(log.entries()[0].message, "5");
        log.push(LogLevel::Warn, "é".repeat(MAX_LOG_MESSAGE));
        let last = &log.entries()[log.len() - 1];
        assert!(last.message.len() <= MAX_LOG_MESSAGE + '…'.len_utf8());
        assert!(last.message.ends_with('…'));
        assert_eq!(log.take().len(), MAX_LOG_ENTRIES);
        assert!(log.is_empty());
    }

    #[test]
    fn an_entry_goes_over_the_wire_and_back() {
        let entry = LogEntry {
            level: LogLevel::Warn,
            at_ms: 42,
            message: "a b".to_owned(),
        };
        let back = LogEntry::from_wire(&entry.to_wire());
        assert_eq!((back.level, back.at_ms), (LogLevel::Warn, 42));
        assert_eq!(back.message, entry.message);
        assert_eq!(LogEntry::from_wire("plain").level, LogLevel::Info);
    }
}
