//! A hot reload of the configuration under test, through the same worker
//! command and host turn a shell's supervisor drives.

use std::sync::{Arc, mpsc};
use std::time::Duration;

use crate::headless::Headless;
use crate::host::turn::{Links, Turn};
use crate::lock::{SupervisorMessage, WorkerCommand};

impl Headless {
    /// Reloads the configuration in place: `source`, or its file read again.
    ///
    /// The reload is handed to the host's turn as the supervisor hands it
    /// to a worker, so a failure keeps the running configuration and shows
    /// what the shell would show. The error is the reload's own.
    pub fn reload(&mut self, source: Option<Vec<u8>>) -> Result<(), String> {
        let screen = self
            .screen_model
            .clone()
            .ok_or("there is no screen to reload onto")?;
        let source = match source {
            Some(source) => source,
            None => std::fs::read(&self.path)
                .map_err(|error| format!("could not read {}: {error}", self.path.display()))?,
        };
        let Some(host) = self.host.as_mut() else {
            return Err("the configuration has no surface to reload into".to_owned());
        };
        let (commands_tx, commands) = mpsc::channel();
        let (tx, supervisor) = mpsc::channel::<SupervisorMessage>();
        let (reply, result) = mpsc::sync_channel(1);
        commands_tx
            .send(WorkerCommand::Reload {
                path: Arc::new(self.path.clone()),
                source: Arc::from(source),
                hard: false,
                reply,
            })
            .map_err(|_| "the reload could not be sent".to_owned())?;
        let links = Links {
            policy: self.policy,
            tx: &tx,
            commands: &commands,
            screen: &screen,
        };
        match host.turn(&mut self.runtime, Some(&links)) {
            Ok(Turn::Again) => {}
            Ok(turn) => self
                .problems
                .push(format!("reload turn ended with {turn:?}")),
            Err(error) => {
                if !self.problems.contains(&error) {
                    self.problems.push(error);
                }
            }
        }
        drop(supervisor);
        self.frame(Duration::ZERO);
        result
            .try_recv()
            .map_err(|_| "the reload was never answered".to_owned())?
    }

    /// The engine's reload-error band as the primary surface last drew it.
    pub fn reload_band(&self) -> Option<&morf_lua::BandLayout> {
        self.host.as_ref()?.state.layout.band.as_ref()
    }
}
