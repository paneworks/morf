//! What a configuration holds in its hand.
//!
//! Every userdata a Lua configuration can be handed -- a signal, a process, a
//! socket, a bus name, a PAM conversation -- is one of these. Split from
//! `state` at the line gate; they belong together anyway, being the whole set
//! of things the engine lets a configuration keep a reference to.

use luna::UserRef;
use morf_system::desktop_entries::DesktopEntries;

use morf_io::{
    DbusProxy, DbusService, FileDocument, FileView, FileWatcher, Process, ServerView, SocketView,
};
use morf_runtime::Handler;
use morf_scene::reactive::SignalId;
use morf_scene::{Easing, GroupId, ListModel, NodeHandle};
use morf_system::menu::Menu;
use morf_system::{GreetdClient, GreetdConversation, PamSession};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

#[derive(Debug)]
pub(crate) struct SignalToken {
    pub(crate) id: SignalId,
}

pub(crate) struct WindowSurfaceToken {
    pub(crate) id: u64,
}

pub(crate) type PopupAnchorArgs<'gc> = (
    UserRef<'gc, WindowSurfaceToken>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
);

pub(crate) type WindowMapRectArgs<'gc> = (
    UserRef<'gc, WindowSurfaceToken>,
    UserRef<'gc, NodeToken>,
    f64,
    f64,
    f64,
    f64,
);

pub(crate) struct TransformWatcherToken {
    pub(crate) id: u64,
}

#[derive(Debug)]
pub(crate) struct NodeToken {
    pub(crate) handle: NodeHandle,
}

pub(crate) struct GroupToken {
    pub(crate) id: GroupId,
}

#[derive(Debug)]
pub(crate) struct DbusToken {
    pub(crate) proxy: DbusProxy,
}

/// What `subscribe` returns: the way to end a subscription.
pub(crate) struct DbusSubscriptionToken {
    pub(crate) id: u64,
}

/// A file descriptor from the bus, held for a configuration that cannot see
/// into it. `None` once closed.
pub(crate) struct DbusFdToken {
    pub(crate) fd: RefCell<Option<morf_io::DbusFd>>,
}

pub(crate) struct DbusServiceToken {
    pub(crate) service: Rc<RefCell<DbusService>>,
}

pub(crate) struct PamSessionToken {
    pub(crate) session: Rc<RefCell<PamSession>>,
}

pub(crate) struct GreetdToken {
    pub(crate) client: RefCell<GreetdClient>,
}

pub(crate) struct GreetdSessionToken {
    pub(crate) conversation: Rc<RefCell<GreetdConversation>>,
}

pub(crate) struct ProcessToken {
    pub(crate) process: RefCell<Process>,
}

pub(crate) struct ProcessViewToken {
    pub(crate) state: RefCell<morf_io::ProcessView>,
}

pub(crate) struct FileToken {
    pub(crate) file: FileView,
}

pub(crate) struct FileWatcherToken {
    pub(crate) watcher: FileWatcher,
}

pub(crate) struct FileDocumentToken {
    pub(crate) file: RefCell<FileDocument>,
}

pub(crate) struct SocketToken {
    pub(crate) state: RefCell<SocketView>,
}

pub(crate) struct SocketServerToken {
    pub(crate) state: RefCell<ServerView>,
}

pub(crate) struct ListModelToken {
    pub(crate) model: Rc<RefCell<ListModel>>,
}

pub(crate) struct ElapsedTimerToken {
    pub(crate) started: RefCell<Instant>,
}

pub(crate) struct EasingCurveToken {
    pub(crate) easing: Easing,
}

pub(crate) struct JsonNullToken;

pub(crate) struct DesktopEntriesToken {
    pub(crate) entries: RefCell<DesktopEntries>,
    pub(crate) paths: Vec<PathBuf>,
}

pub(crate) struct MenuToken {
    pub(crate) menu: RefCell<Menu>,
    pub(crate) callbacks: HashMap<String, Handler>,
}

/// A handle to a `morf.time.timer`, good for one thing: stopping it.
pub(crate) struct TimerToken {
    pub(crate) id: u64,
}
