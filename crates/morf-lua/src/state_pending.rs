//! Native work in flight, and who to tell when it finishes.
//!
//! Split from `state` at the line gate, and these belong together anyway: each
//! is a job the engine is running on a configuration's behalf -- a timer, an
//! authentication, a bus name, a subscription -- paired with the closure that
//! is owed the answer. The runtime drains all of them in one pass.

use luna::StashedClosure;
use morf_io::{DbusService, DbusSignal, FileWatcher, PendingReply, Timer as IoTimer};
use morf_reactive::SignalId;
use morf_scene::NodeHandle;
use morf_services::{GreetdConversation, PamSession, PamTask, StatusNotifierHost, UdevMonitor};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

pub(crate) struct PendingPam {
    pub(crate) task: PamTask,
    pub(crate) callback: StashedClosure,
    pub(crate) unlock_on_success: bool,
}

pub(crate) struct PendingTimer {
    /// Names the timer to whoever holds its handle, so it can be cancelled.
    pub(crate) id: u64,
    pub(crate) timer: IoTimer,
    pub(crate) callback: StashedClosure,
    pub(crate) repeat: bool,
    pub(crate) interval: Duration,
    pub(crate) node: Option<NodeHandle>,
}

pub(crate) struct PendingDbusSignal {
    /// Names the subscription to the handle `subscribe` returned, so it can
    /// be closed.
    pub(crate) id: u64,
    pub(crate) signal: DbusSignal,
    pub(crate) callback: StashedClosure,
    pub(crate) kind: DbusSignalKind,
}

/// How a subscription's callback is called.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DbusSignalKind {
    /// `callback(body, info)`: the body as it always was, then a table with
    /// the sender, the address and the body again.
    Signal,
    /// `callback(old_owner, new_owner, name)`, from `on_name_owner_changed`.
    OwnerChanged,
}

/// A method call made with `call_async`, and who is owed the answer.
pub(crate) struct PendingDbusReply {
    pub(crate) reply: PendingReply,
    pub(crate) callback: StashedClosure,
}

/// A bus name this configuration owns, and who answers calls on it.
///
/// The service is shared rather than owned here because it is reachable from
/// two directions at once: the runtime polls it for arriving calls, and the
/// configuration replies through the same handle from inside the callback
/// those calls are delivered to.
pub(crate) struct PendingDbusService {
    pub(crate) service: Rc<RefCell<DbusService>>,
    pub(crate) callback: StashedClosure,
}

/// A PAM conversation in progress, and who is shown its messages.
///
/// Shared with the token for the same reason the D-Bus service is: the runtime
/// polls it for what the module said, and the configuration answers through
/// the same handle from inside the callback that showed it the question.
pub(crate) struct PendingPamSession {
    pub(crate) session: Rc<RefCell<PamSession>>,
    pub(crate) callback: StashedClosure,
}

/// A greetd login in progress, and the configuration listening to it.
pub(crate) struct PendingGreetdSession {
    pub(crate) conversation: Rc<RefCell<GreetdConversation>>,
    pub(crate) callback: StashedClosure,
}

pub(crate) struct PendingUdev {
    pub(crate) monitor: UdevMonitor,
    pub(crate) callback: StashedClosure,
}

pub(crate) struct PendingStatusNotifier {
    pub(crate) host: StatusNotifierHost,
    pub(crate) callback: StashedClosure,
}

/// A JSON file a theme takes its tokens from, watched for rewrites.
pub(crate) struct ThemeSource {
    pub(crate) path: PathBuf,
    /// Absent when the directory could not be watched; the file is then read
    /// once and never again.
    pub(crate) watcher: Option<FileWatcher>,
    /// The token each leaf key of the file writes.
    pub(crate) fields: HashMap<String, SignalId>,
}

/// The signals behind `morf.prefers`, and the settings portal they follow.
pub(crate) struct Prefers {
    pub(crate) color_scheme: SignalId,
    pub(crate) contrast: SignalId,
    pub(crate) reduced_motion: SignalId,
    pub(crate) accent_color: SignalId,
    pub(crate) scale: SignalId,
    /// The settings portal, followed without ever being waited on; `None`
    /// when there is no session bus to find one on.
    pub(crate) portal: Option<PortalWatch>,
    /// Preferences the host set itself (`set_preference`), which a portal
    /// reading that was already on its way must not overwrite.
    pub(crate) overridden: HashSet<&'static str>,
}

/// How `morf.prefers` follows the settings portal.
///
/// Nothing here blocks: the portal is asked only once it has an owner — a
/// read of an absent portal would activate it, and activating a portal can
/// take the whole of a call's timeout — and its answers are collected from a
/// poll.
pub(crate) struct PortalWatch {
    /// `SettingChanged`, from whoever owns the portal's name.
    pub(crate) changes: DbusSignal,
    /// The portal's name changing hands: a portal that starts after the
    /// shell is read when it arrives.
    pub(crate) owner: DbusSignal,
    /// Readings asked for and not yet answered: namespace, key, reply.
    pub(crate) pending: Vec<(&'static str, &'static str, PendingReply)>,
}
