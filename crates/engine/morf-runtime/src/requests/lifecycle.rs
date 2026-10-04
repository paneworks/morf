//! What a configuration asks of its own life: a reload, file watching, to
//! quit, to unlock; and who hears how a reload went.

use crate::handler::Handler;

/// A reload, file watching, quitting and unlocking, as asked.
pub struct Lifecycle {
    /// A reload asked for: `Some(hard)`.
    pub reload_request: Option<bool>,
    /// Whether the configuration's files are watched for a reload.
    pub watch_files: bool,
    /// Whether that changed since the host last asked.
    pub watch_files_changed: bool,
    /// Whether the configuration has asked the shell to stop.
    ///
    /// One-way: nothing clears it but the supervisor reading it, and by then
    /// the process is on its way out. A configuration cannot un-quit.
    pub quit_requested: bool,
    /// Whether the lock screen asked to unlock.
    pub session_unlock_requested: bool,
    /// Told when a reload finished.
    pub reload_completed_callbacks: Vec<Handler>,
    /// Told when a reload failed.
    pub reload_failed_callbacks: Vec<Handler>,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            reload_request: None,
            watch_files: true,
            watch_files_changed: false,
            quit_requested: false,
            session_unlock_requested: false,
            reload_completed_callbacks: Vec::new(),
            reload_failed_callbacks: Vec::new(),
        }
    }
}
