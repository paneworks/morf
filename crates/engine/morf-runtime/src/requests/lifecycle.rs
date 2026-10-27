//! What a configuration asks of its own life: a reload, file watching, to
//! quit, to unlock; who hears how a reload went; and the values it keeps
//! across one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use morf_scene::reactive::SignalId;
use morf_value::IpcValue;

use crate::handler::Handler;

/// A reload a configuration asked for.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReloadRequest {
    /// Its surfaces made again as well, not only its runtime.
    pub hard: bool,
    /// Only the runtime of the output that asked, not every output's.
    pub output: bool,
}

/// A reload and quitting, as asked.
pub struct Lifecycle {
    /// A reload asked for.
    reload_request: Option<ReloadRequest>,
    /// Whether the configuration has asked the shell to stop.
    ///
    /// One-way: nothing clears it but the supervisor reading it, and by then
    /// the process is on its way out. A configuration cannot un-quit.
    quit_requested: bool,
    /// Told when a reload finished or failed.
    reload_callbacks: Vec<Handler>,
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
            quit_requested: false,
            reload_callbacks: Vec::new(),
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

    /// Asks for a reload: a hard one wins over a soft one, and one of every
    /// output over one of the asking output alone.
    pub fn ask_reload(&mut self, asked: ReloadRequest) {
        self.reload_request = Some(match self.reload_request {
            Some(earlier) => ReloadRequest {
                hard: earlier.hard || asked.hard,
                output: earlier.output && asked.output,
            },
            None => asked,
        });
    }

    /// The reload asked for, if one was.
    pub fn take_reload_request(&mut self) -> Option<ReloadRequest> {
        self.reload_request.take()
    }

    /// The configuration asked the shell to stop.
    pub fn quit(&mut self) {
        self.quit_requested = true;
    }

    /// Whether the configuration has asked the shell to stop.
    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    /// Tells `handler` when a reload finishes or fails.
    pub fn on_reload(&mut self, handler: Handler) {
        self.reload_callbacks.push(handler);
    }

    /// Who is told that a reload finished or failed.
    pub fn reload_callbacks(&self) -> Vec<Handler> {
        self.reload_callbacks.clone()
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
