//! What to start: a child's command and pipes, or a socket's endpoint.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use super::DEFAULT_MAX_LINE;

/// What a child's standard input is.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum StdinMode {
    /// `/dev/null`.
    #[default]
    Null,
    /// These bytes, then end of file.
    Data(Vec<u8>),
    /// Kept open for [`super::ReactorControl::write`] until closed.
    Pipe,
}

/// What becomes of a child's stdout or stderr.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutputMode {
    /// Read and delivered as events.
    #[default]
    Capture,
    /// `/dev/null`.
    Null,
    /// The same stream morf itself writes to.
    Inherit,
}

/// How to start a child. Never through a shell: `command` is the argv.
#[derive(Clone, Debug)]
pub struct SpawnOptions {
    pub command: Vec<String>,
    /// Set in the child on top of what it inherits.
    pub environment: BTreeMap<String, String>,
    /// Start from an empty environment instead of morf's.
    pub clear_environment: bool,
    pub working_directory: Option<PathBuf>,
    pub stdin: StdinMode,
    pub stdout: OutputMode,
    pub stderr: OutputMode,
    /// Split output on `\n` and deliver lines rather than chunks.
    pub lines: bool,
    pub max_line: usize,
    /// Output past this many bytes, both streams together, is read and
    /// dropped; the exit then says `truncated`.
    pub max_output: Option<usize>,
    /// Terminated when it runs longer, killed if it will not go.
    pub timeout: Option<Duration>,
    /// Left running when the reactor goes, in a process group of its own.
    pub detached: bool,
}

impl SpawnOptions {
    pub fn new(command: Vec<String>) -> Self {
        Self {
            command,
            environment: BTreeMap::new(),
            clear_environment: false,
            working_directory: None,
            stdin: StdinMode::Null,
            stdout: OutputMode::Capture,
            stderr: OutputMode::Capture,
            lines: true,
            max_line: DEFAULT_MAX_LINE,
            max_output: None,
            timeout: None,
            detached: false,
        }
    }
}

/// Where a connection goes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Endpoint {
    Unix(PathBuf),
    Tcp { host: String, port: u16 },
}

/// How to open a connection.
#[derive(Clone, Debug)]
pub struct ConnectOptions {
    pub endpoint: Endpoint,
    pub lines: bool,
    pub max_line: usize,
    /// Given up with [`super::CloseReason::TimedOut`] when not connected by then.
    pub connect_timeout: Duration,
    /// The whole connection's life, for a request that must be answered.
    pub deadline: Option<Duration>,
    /// Written as soon as the connection is up.
    pub greeting: Vec<u8>,
}

impl ConnectOptions {
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            lines: false,
            max_line: DEFAULT_MAX_LINE,
            connect_timeout: Duration::from_secs(5),
            deadline: None,
            greeting: Vec::new(),
        }
    }
}
