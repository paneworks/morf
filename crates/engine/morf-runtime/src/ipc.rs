//! The verbs a configuration answers over IPC (`morf.ipc`), by name.

use std::collections::HashMap;

use crate::Handler;

#[derive(Default)]
pub struct IpcVerbs {
    handlers: HashMap<String, Handler>,
}

impl IpcVerbs {
    /// Answers `verb` with `handler`, replacing any it had.
    pub fn set(&mut self, verb: String, handler: Handler) {
        self.handlers.insert(verb, handler);
    }

    /// Stops answering `verb`.
    pub fn remove(&mut self, verb: &str) {
        self.handlers.remove(verb);
    }

    /// The handler answering `verb`.
    pub fn get(&self, verb: &str) -> Option<&Handler> {
        self.handlers.get(verb)
    }

    /// Every verb answered, in lexical order.
    pub fn names(&self) -> Vec<String> {
        let mut names = self.handlers.keys().cloned().collect::<Vec<_>>();
        names.sort();
        names
    }
}
