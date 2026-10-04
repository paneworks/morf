//! Native work in flight, and who to tell when it finishes.
//!
//! Split from `state` at the line gate, and these belong together anyway: each
//! is a job the engine is running on a configuration's behalf -- a timer, an
//! authentication, a bus name, a subscription -- paired with the closure that
//! is owed the answer. The runtime drains all of them in one pass.

use morf_io::{DbusSignal, FileWatcher, PendingReply};
use morf_runtime::Handler;
use morf_scene::reactive::SignalId;
use morf_system::{GreetdConversation, PamSession, PamTask, StatusNotifierHost, UdevMonitor};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

pub(crate) struct PendingPam {
    pub(crate) task: PamTask,
    pub(crate) callback: Handler,
    pub(crate) unlock_on_success: bool,
}

pub(crate) struct PendingDbusSignal {
    /// Names the subscription to the handle `subscribe` returned, so it can
    /// be closed.
    pub(crate) id: u64,
    pub(crate) signal: DbusSignal,
    pub(crate) callback: Handler,
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
    pub(crate) callback: Handler,
}

/// A PAM conversation in progress, and who is shown its messages.
///
/// Shared with the token for the same reason the D-Bus service is: the runtime
/// polls it for what the module said, and the configuration answers through
/// the same handle from inside the callback that showed it the question.
pub(crate) struct PendingPamSession {
    pub(crate) session: Rc<RefCell<PamSession>>,
    pub(crate) callback: Handler,
}

/// A greetd login in progress, and the configuration listening to it.
pub(crate) struct PendingGreetdSession {
    pub(crate) conversation: Rc<RefCell<GreetdConversation>>,
    pub(crate) callback: Handler,
}

pub(crate) struct PendingUdev {
    pub(crate) monitor: UdevMonitor,
    pub(crate) callback: Handler,
}

pub(crate) struct PendingStatusNotifier {
    pub(crate) host: StatusNotifierHost,
    pub(crate) callback: Handler,
}

/// A JSON file a theme takes its tokens from, watched for rewrites.
/// A theme token's colour on its way to the one last written to it.
pub(crate) struct ThemeFade {
    pub(crate) signal: morf_scene::reactive::SignalId,
    pub(crate) from: morf_scene::Color,
    pub(crate) to: morf_scene::Color,
    pub(crate) elapsed: std::time::Duration,
    pub(crate) duration: std::time::Duration,
    pub(crate) easing: morf_scene::Easing,
}

impl ThemeFade {
    /// The colour on show now, and whether the fade is over.
    pub(crate) fn colour(&self) -> (morf_scene::Color, bool) {
        let progress = if self.duration.is_zero() {
            1.0
        } else {
            (self.elapsed.as_secs_f64() / self.duration.as_secs_f64()).min(1.0)
        };
        let colour = self.easing.interpolate_color(
            progress,
            self.from,
            self.to,
            morf_scene::ColorSpace::Oklab,
            morf_scene::HueDirection::Shorter,
        );
        (colour, progress >= 1.0)
    }
}

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
    pub(crate) portal: Option<morf_system::prefers::Portal>,
    /// Preferences the host set itself (`set_preference`), which a portal
    /// reading that was already on its way must not overwrite.
    pub(crate) overridden: HashSet<&'static str>,
}

