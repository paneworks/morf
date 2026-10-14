//! What a configuration asks of its own life: a reload, file watching, to
//! quit, to unlock; who hears how a reload went; and the values it keeps
//! across one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use morf_scene::reactive::SignalId;
use morf_value::IpcValue;

use crate::handler::Handler;

/// A reload, file watching, quitting and unlocking, as asked.
pub struct Lifecycle {
    /// A reload asked for: `Some(hard)`.
    reload_request: Option<bool>,
    /// Whether the configuration's files are watched for a reload.
    watch_files: bool,
    /// Whether that changed since the host last asked.
    watch_files_changed: bool,
    /// Whether the configuration has asked the shell to stop.
    ///
    /// One-way: nothing clears it but the supervisor reading it, and by then
    /// the process is on its way out. A configuration cannot un-quit.
    quit_requested: bool,
    /// Whether the lock screen asked to unlock.
    session_unlock_requested: bool,
    /// Told when a reload finished.
    reload_completed_callbacks: Vec<Handler>,
    /// Told when a reload failed.
    reload_failed_callbacks: Vec<Handler>,
    /// What the last configuration kept, waiting to be claimed by name.
    reload_seed: HashMap<String, IpcValue>,
    /// Each `morf.reloadable` value, by name.
    reloadable: HashMap<String, SignalId>,
    /// Where the configuration's own files are.
    shell_root: PathBuf,
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
            reload_seed: HashMap::new(),
            reloadable: HashMap::new(),
            shell_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }
}

impl Lifecycle {
    /// Where the configuration's own files are.
    pub fn shell_root(&self) -> &Path {
        &self.shell_root
    }

    /// Roots the configuration at `root`.
    pub fn set_shell_root(&mut self, root: PathBuf) {
        self.shell_root = root;
    }

    /// Asks for a reload; a hard one wins over a soft one.
    pub fn ask_reload(&mut self, hard: bool) {
        self.reload_request = Some(self.reload_request.unwrap_or(false) || hard);
    }

    /// The reload asked for, if one was: `Some(hard)`.
    pub fn take_reload_request(&mut self) -> Option<bool> {
        self.reload_request.take()
    }

    /// Whether the configuration's files are watched.
    pub fn watch_files(&self) -> bool {
        self.watch_files
    }

    /// Watches the configuration's files, or stops.
    pub fn set_watch_files(&mut self, watch: bool) {
        if self.watch_files != watch {
            self.watch_files = watch;
            self.watch_files_changed = true;
        }
    }

    /// Whether to watch the files, if that changed since last asked.
    pub fn take_watch_files_change(&mut self) -> Option<bool> {
        std::mem::take(&mut self.watch_files_changed).then_some(self.watch_files)
    }

    /// The configuration asked the shell to stop.
    pub fn quit(&mut self) {
        self.quit_requested = true;
    }

    /// Whether the configuration has asked the shell to stop.
    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    /// The lock screen asked to unlock.
    pub fn ask_unlock(&mut self) {
        self.session_unlock_requested = true;
    }

    /// Whether the lock screen asked to unlock since last asked.
    pub fn take_unlock_request(&mut self) -> bool {
        std::mem::take(&mut self.session_unlock_requested)
    }

    /// Tells `handler` when a reload finishes, or when one fails.
    pub fn on_reload(&mut self, completed: bool, handler: Handler) {
        if completed {
            self.reload_completed_callbacks.push(handler);
        } else {
            self.reload_failed_callbacks.push(handler);
        }
    }

    /// Who is told that a reload finished, or failed.
    pub fn reload_callbacks(&self, completed: bool) -> Vec<Handler> {
        if completed {
            self.reload_completed_callbacks.clone()
        } else {
            self.reload_failed_callbacks.clone()
        }
    }

    /// What the last configuration kept, for this one to claim.
    pub fn seed(&mut self, values: impl IntoIterator<Item = (String, IpcValue)>) {
        self.reload_seed = values.into_iter().collect();
    }

    /// Claims what the last configuration kept under `name`.
    pub fn take_seed(&mut self, name: &str) -> Option<IpcValue> {
        self.reload_seed.remove(name)
    }

    /// Whether a value is kept under `name` already.
    pub fn is_reloadable(&self, name: &str) -> bool {
        self.reloadable.contains_key(name)
    }

    /// Keeps `signal`'s value across reloads, under `name`.
    pub fn keep(&mut self, name: String, signal: SignalId) {
        self.reloadable.insert(name, signal);
    }

    /// Every value kept across reloads, by name.
    pub fn reloadables(&self) -> impl Iterator<Item = (&str, SignalId)> {
        self.reloadable
            .iter()
            .map(|(name, signal)| (name.as_str(), *signal))
    }
}
