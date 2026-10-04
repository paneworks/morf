//! A child described by its configuration, started, stopped and restarted
//! as that configuration changes.
//!
//! [`Process`] is one child; a [`ProcessView`] is the description a script
//! edits — command, environment, working directory — plus the child it is
//! running, if any. Changing the description of a running view respawns
//! the child with the new one, and only adopts it once the spawn worked, so
//! a bad edit leaves both the old description and the old child in place.

use std::path::PathBuf;
use std::time::Duration;

use crate::{Process, ProcessConfig, ProcessEvent};

/// The longest working directory a view accepts, in bytes.
pub const MAX_WORKING_DIRECTORY: usize = 4_096;

/// A child's description and the child itself while it runs.
pub struct ProcessView {
    config: ProcessConfig,
    process: Option<Process>,
}

impl ProcessView {
    /// A view of `config`, its child started when `running`.
    pub fn new(config: ProcessConfig, running: bool) -> Result<Self, String> {
        check_command(&config.command)?;
        let process = running
            .then(|| Process::spawn_config(&config))
            .transpose()
            .map_err(|error| error.to_string())?;
        Ok(Self { config, process })
    }

    pub fn config(&self) -> &ProcessConfig {
        &self.config
    }

    /// Starts the child; false when it was already running.
    pub fn start(&mut self) -> Result<bool, String> {
        if self.process.is_some() {
            return Ok(false);
        }
        self.process =
            Some(Process::spawn_config(&self.config).map_err(|error| error.to_string())?);
        Ok(true)
    }

    pub fn running(&self) -> bool {
        self.process.is_some()
    }

    /// The running child's process id.
    pub fn id(&self) -> Option<u32> {
        self.process.as_ref().map(Process::id)
    }

    fn child(&mut self) -> Result<&mut Process, String> {
        self.process
            .as_mut()
            .ok_or_else(|| "process is not running".to_owned())
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.child()?
            .write(bytes)
            .map_err(|error| error.to_string())
    }

    /// Closes the child's stdin; nothing when it is not running.
    pub fn close_stdin(&mut self) {
        if let Some(process) = self.process.as_mut() {
            process.close_stdin();
        }
    }

    pub fn kill(&mut self) -> Result<(), String> {
        self.child()?.kill().map_err(|error| error.to_string())
    }

    pub fn signal(&mut self, signal: i32) -> Result<(), String> {
        self.child()?
            .signal(signal)
            .map_err(|error| error.to_string())
    }

    /// The child's next event within `timeout`; after its exit the view is
    /// no longer running.
    pub fn next_event(&mut self, timeout: Duration) -> Result<Option<ProcessEvent>, String> {
        let event = self
            .child()?
            .next_event(timeout)
            .map_err(|error| error.to_string())?;
        if matches!(event, Some(ProcessEvent::Exit(_))) {
            self.process = None;
        }
        Ok(event)
    }

    /// Edits the description; a running child is respawned with the new
    /// one, and nothing changes when that spawn fails.
    pub fn update(&mut self, edit: impl FnOnce(&mut ProcessConfig)) -> Result<(), String> {
        let mut config = self.config.clone();
        edit(&mut config);
        check_command(&config.command)?;
        let replacement = self
            .process
            .is_some()
            .then(|| Process::spawn_config(&config))
            .transpose()
            .map_err(|error| error.to_string())?;
        self.config = config;
        if let Some(replacement) = replacement {
            self.process = Some(replacement);
        }
        Ok(())
    }
}

/// Refuses a view with nothing to run.
pub fn check_command(command: &[String]) -> Result<(), String> {
    if command.is_empty() {
        return Err("process_view command cannot be empty".into());
    }
    Ok(())
}

/// A working directory as a script gives it, refused when empty, longer
/// than [`MAX_WORKING_DIRECTORY`] or holding a NUL.
pub fn working_directory(directory: String) -> Result<PathBuf, String> {
    if directory.is_empty()
        || directory.len() > MAX_WORKING_DIRECTORY
        || directory.as_bytes().contains(&0)
    {
        return Err("working directory is invalid".into());
    }
    Ok(PathBuf::from(directory))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(command: &[&str]) -> ProcessConfig {
        ProcessConfig {
            command: command.iter().map(|part| part.to_string()).collect(),
            ..ProcessConfig::default()
        }
    }

    #[test]
    fn a_view_refuses_an_empty_command() {
        assert!(ProcessView::new(config(&[]), false).is_err());
        let mut view = ProcessView::new(config(&["true"]), false).unwrap();
        assert!(view.update(|config| config.command.clear()).is_err());
        assert_eq!(view.config().command, vec!["true".to_string()]);
    }

    #[test]
    fn a_stopped_view_starts_and_runs_to_exit() {
        let mut view = ProcessView::new(config(&["true"]), false).unwrap();
        assert!(!view.running());
        assert!(view.write(b"x").is_err());
        assert!(view.start().unwrap());
        assert!(!view.start().unwrap());
        assert!(view.id().is_some());
        let mut exited = false;
        for _ in 0..200 {
            match view.next_event(Duration::from_millis(50)).unwrap() {
                Some(ProcessEvent::Exit(status)) => {
                    assert!(status.success());
                    exited = true;
                    break;
                }
                _ => continue,
            }
        }
        assert!(exited && !view.running());
    }

    #[test]
    fn working_directories_are_checked() {
        assert!(working_directory(String::new()).is_err());
        assert!(working_directory("a\0b".into()).is_err());
        assert!(working_directory("x".repeat(MAX_WORKING_DIRECTORY + 1)).is_err());
        assert_eq!(
            working_directory("/tmp".into()).unwrap(),
            PathBuf::from("/tmp")
        );
    }
}
