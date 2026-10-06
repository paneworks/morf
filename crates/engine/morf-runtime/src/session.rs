//! What the host tells a runtime about its place in the session, each as a
//! tracked signal: where the session lock stands, whether this runtime is
//! the primary one of its process, and how many times the output list has
//! changed.

use morf_scene::reactive::SignalId;
use morf_value::IpcValue;

use crate::Handler;
use crate::reactive::Reactive;
use crate::screens::Screen;

/// One step of an ext-session-lock-v1 lock's life.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionLockState {
    /// No lock is held by this process.
    Unlocked,
    /// The lock was asked for and the compositor has not answered yet.
    Pending,
    /// The compositor confirmed the lock (`locked`): the session is hidden.
    Locked,
    /// The compositor refused the lock (`finished` before `locked`).
    Failed,
}

impl SessionLockState {
    /// The name Lua sees.
    pub fn name(self) -> &'static str {
        match self {
            Self::Unlocked => "unlocked",
            Self::Pending => "pending",
            Self::Locked => "locked",
            Self::Failed => "failed",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        [Self::Unlocked, Self::Pending, Self::Locked, Self::Failed]
            .into_iter()
            .find(|state| state.name() == name)
    }
}

/// The session's signals, and what each holds.
pub struct Session {
    /// `morf.session_lock`: the lock's state by name.
    pub lock: SignalId,
    /// `morf.primary()`: its signal, and whether this runtime is primary.
    pub primary: Option<(SignalId, bool)>,
    /// `morf.screens_revision()`: its signal, and how many times the output
    /// list has changed.
    pub screens_revision: Option<(SignalId, i64)>,
    /// What the output list last was ([`screens_signature`]).
    pub screens_signature: String,
    /// Told when the lock changes, each with whether it wants only `locked`.
    pub session_lock_callbacks: Vec<(Handler, bool)>,
    /// `morf.lock_surface`: builds one output's lock tree, given its screen.
    pub lock_surface_builder: Option<Handler>,
    /// `morf.on_primary(fn)`: called with the new value when it changes.
    pub primary_callbacks: Vec<Handler>,
}

/// Writes a signal the host set, through the graph when it is in place.
fn write(reactive: &mut Reactive, signal: SignalId, value: IpcValue) -> Result<(), String> {
    let written = match reactive.graph.as_mut() {
        Some(graph) => graph
            .write(signal, value.clone())
            .map(|_| ())
            .map_err(|error| error.to_string()),
        None => Ok(()),
    };
    reactive.values.insert(signal, value);
    written
}

impl Session {
    pub fn new(lock: SignalId) -> Self {
        Self {
            lock,
            primary: None,
            screens_revision: None,
            screens_signature: String::new(),
            session_lock_callbacks: Vec::new(),
            lock_surface_builder: None,
            primary_callbacks: Vec::new(),
        }
    }

    /// Where the lock stands, as last recorded.
    pub fn lock_state(&self, reactive: &Reactive) -> SessionLockState {
        match reactive.values.get(&self.lock) {
            Some(IpcValue::String(name)) => {
                SessionLockState::parse(name).unwrap_or(SessionLockState::Unlocked)
            }
            _ => SessionLockState::Unlocked,
        }
    }

    /// Records the lock's new state. Returns the value written, or `None`
    /// when nothing changed; an error is the graph's, the value written
    /// anyway.
    pub fn set_lock_state(
        &self,
        reactive: &mut Reactive,
        next: SessionLockState,
    ) -> Option<(IpcValue, Result<(), String>)> {
        if self.lock_state(reactive) == next {
            return None;
        }
        let value = IpcValue::String(next.name().to_owned());
        let written = write(reactive, self.lock, value.clone());
        Some((value, written))
    }

    /// Whether this runtime is the primary one. A runtime nobody told
    /// otherwise is.
    pub fn is_primary(&self) -> bool {
        self.primary.is_none_or(|(_, primary)| primary)
    }

    /// Makes this runtime primary, or not. Returns the value written, or
    /// `None` when nothing changed (or there is no signal to write).
    pub fn set_primary(
        &mut self,
        reactive: &mut Reactive,
        primary: bool,
    ) -> Option<(IpcValue, Result<(), String>)> {
        if self.is_primary() == primary {
            return None;
        }
        let (signal, _) = self.primary?;
        self.primary = Some((signal, primary));
        let value = IpcValue::Boolean(primary);
        let written = write(reactive, signal, value.clone());
        Some((value, written))
    }

    /// The output list is now `screens`: moves the revision on when it is
    /// not what it was. Returns whether it moved.
    pub fn screens_changed(&mut self, reactive: &mut Reactive, screens: &[Screen]) -> bool {
        let signature = screens_signature(screens);
        if self.screens_signature == signature {
            return false;
        }
        self.screens_signature = signature;
        let Some((signal, count)) = self.screens_revision else {
            return false;
        };
        let count = count + 1;
        self.screens_revision = Some((signal, count));
        let value = IpcValue::Integer(count);
        if let Some(graph) = reactive.graph.as_mut()
            && graph.write(signal, value.clone()).is_ok()
        {
            reactive.values.insert(signal, value);
        }
        true
    }
}

impl Session {
    /// Moves `morf.screens_revision` though the outputs are the same: what is
    /// read off them changed (their size in morf's pixels, with the density).
    pub fn touch_screens(&mut self, reactive: &mut Reactive) {
        let Some((signal, count)) = self.screens_revision else {
            return;
        };
        let count = count + 1;
        self.screens_revision = Some((signal, count));
        let value = IpcValue::Integer(count);
        if let Some(graph) = reactive.graph.as_mut()
            && graph.write(signal, value.clone()).is_ok()
        {
            reactive.values.insert(signal, value);
        }
    }
}

/// What makes two output lists the same for `morf.screens_revision`.
pub fn screens_signature(screens: &[Screen]) -> String {
    screens
        .iter()
        .map(screen_signature)
        .collect::<Vec<_>>()
        .join(";")
}

/// What makes one output the same.
pub fn screen_signature(screen: &Screen) -> String {
    format!(
        "{}|{:?}|{:?}x{:?}|{}|{}",
        screen.name, screen.position, screen.width, screen.height, screen.scale, screen.transform
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use morf_scene::reactive::Graph;

    fn session() -> (Reactive, Session) {
        let mut graph = Graph::default();
        let lock = graph.signal("lock", IpcValue::String("unlocked".to_owned()));
        let primary = graph.signal("primary", IpcValue::Boolean(true));
        let screens = graph.signal("screens", IpcValue::Integer(0));
        let mut session = Session::new(lock);
        session.primary = Some((primary, true));
        session.screens_revision = Some((screens, 0));
        let reactive = Reactive {
            graph: Some(graph),
            ..Reactive::default()
        };
        (reactive, session)
    }

    #[test]
    fn the_lock_state_is_written_once_per_change() {
        let (mut reactive, session) = session();
        assert_eq!(session.lock_state(&reactive), SessionLockState::Unlocked);
        let (value, written) = session
            .set_lock_state(&mut reactive, SessionLockState::Pending)
            .unwrap();
        assert_eq!(value, IpcValue::String("pending".to_owned()));
        assert!(written.is_ok());
        assert_eq!(session.lock_state(&reactive), SessionLockState::Pending);
        assert!(
            session
                .set_lock_state(&mut reactive, SessionLockState::Pending)
                .is_none()
        );
    }

    #[test]
    fn the_primary_duty_moves_and_a_runtime_nobody_told_is_primary() {
        let (mut reactive, mut session) = session();
        assert!(Session::new(session.lock).is_primary());
        assert!(session.set_primary(&mut reactive, true).is_none());
        let (value, _) = session.set_primary(&mut reactive, false).unwrap();
        assert_eq!(value, IpcValue::Boolean(false));
        assert!(!session.is_primary());
    }

    #[test]
    fn the_screens_revision_moves_only_when_the_list_does() {
        let (mut reactive, mut session) = session();
        let one = Screen {
            name: "A".to_owned(),
            scale: 1,
            ..Screen::default()
        };
        assert!(session.screens_changed(&mut reactive, std::slice::from_ref(&one)));
        assert!(!session.screens_changed(&mut reactive, std::slice::from_ref(&one)));
        let two = Screen {
            scale: 2,
            ..one.clone()
        };
        assert!(session.screens_changed(&mut reactive, &[two]));
        assert_eq!(session.screens_revision.map(|(_, count)| count), Some(2));
    }
}
