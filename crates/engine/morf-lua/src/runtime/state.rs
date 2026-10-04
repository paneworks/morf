//! `ReactiveState`, everything one runtime holds between turns, and the
//! types its fields are made of.

pub(crate) use crate::api_shader::RegisteredShader;
use crate::states::{Capture, StateSet};
use luna::StashedTable;

use morf_layout::{TransformTracker, TransformWatcher as NativeTransformWatcher};
use morf_runtime::Handler;
use morf_scene::reactive::{Graph, SignalId};
use morf_scene::{ListModel, NodeHandle, Scene};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use crate::{surface_types::*, types::LogLevel};
// Re-exported, because these moved out of this file only to satisfy the line
// gate: every consumer reaches for them through `state::*` and there is no
// reason to make them all learn a second module name.
pub(crate) use crate::state_pending::*;
pub(crate) use crate::state_tokens::*;

mod follow;
mod methods;

pub(crate) use follow::apply_follows;
pub(crate) use morf_runtime::animation::Follow;

pub(crate) use morf_runtime::views::{Delegate, VirtualView};

/// A `morf.state` table: each named field its own signal, each nested
/// table its own proxy, each array a list model.
pub(crate) struct StateToken {
    pub(crate) fields: Rc<RefCell<StateFields>>,
}

#[derive(Default)]
pub(crate) struct StateFields {
    pub(crate) scalars: HashMap<String, SignalId>,
    /// A field computed from the others on every read: a theme's derived
    /// token. Read inside a binding, whatever it reads is what the binding
    /// tracks, so it re-derives exactly when its inputs change.
    pub(crate) derived: HashMap<String, Handler>,
    /// A theme: a string written to a field that names a colour becomes one.
    pub(crate) theme: bool,
    /// A theme's `transition`: a colour written to a token eases there from
    /// the one on show, over this long, on this curve.
    pub(crate) transition: Option<(std::time::Duration, morf_scene::Easing)>,
    pub(crate) tables: HashMap<String, luna::StashedUserData>,
    pub(crate) lists: HashMap<String, (luna::StashedUserData, Rc<RefCell<ListModel>>)>,
}

/// What a `ui.Layout` container answers layout with.
#[derive(Clone)]
pub(crate) struct CustomLayoutFns {
    pub(crate) measure: Handler,
    pub(crate) place: Handler,
}

#[derive(Clone, Copy)]
pub(crate) enum ViewKind {
    Repeater,
    List,
    Grid,
}

pub(crate) struct LuaTransformWatcher {
    pub(crate) a: NodeHandle,
    pub(crate) b: NodeHandle,
    pub(crate) watcher: NativeTransformWatcher,
    pub(crate) callback: Option<Handler>,
    pub(crate) revision: u64,
    pub(crate) pending: bool,
}

pub(crate) use morf_runtime::windows::PopupNodeAnchor;

pub(crate) use morf_runtime::reactive::Effect as LuaEffect;

pub(crate) use morf_runtime::retention::RetainCallbacks;

pub(crate) use morf_runtime::reactive::{EffectSink, PropertySink};

#[cfg(test)]
pub(crate) use morf_runtime::log::MAX_LOG_ENTRIES;

pub(crate) struct ReactiveState {
    /// The runtime's limits, for a binding made where they are not at hand
    /// (a function assigned to a node's property).
    pub(crate) limits: crate::Limits,
    /// The graph, the signal mirror, the declared effects and the flush.
    pub(crate) reactive: morf_runtime::reactive::Reactive,
    pub(crate) property_signals: HashMap<(NodeHandle, String, bool), SignalId>,
    pub(crate) current_property_names: HashMap<String, (NodeHandle, String)>,
    /// How far the scene has moved, and how far each reader of it has seen.
    pub(crate) revisions: morf_runtime::reactive::Revisions,
    /// Each list model a binding has read, by address, with the revision
    /// signal that binding depends on.
    pub(crate) model_revisions: morf_runtime::models::ModelRevisions,
    /// `morf.shared` values: this copy's signals for them, and what to publish.
    pub(crate) shared: crate::shared::SharedValues,
    pub(crate) channels: crate::channels::Channels,
    pub(crate) reload_seed: HashMap<String, IpcValue>,
    pub(crate) reloadable: HashMap<String, SignalId>,
    /// A reload, file watching, quitting and unlocking, as asked.
    pub(crate) lifecycle: morf_runtime::requests::Lifecycle,
    /// Nodes the lint has already complained about, so a bar that paints
    /// sixty times a second says it once.
    pub(crate) lint_warned: HashSet<NodeHandle>,
    /// What this output's compositor and GPU can do, as name = value pairs.
    /// Filled once the connection is up; read by `morf.capabilities` and by
    /// `morf info`.
    pub(crate) capabilities: Vec<(String, String)>,
    pub(crate) next_effect: u64,
    pub(crate) active: Option<Capture>,
    /// How many Lua handlers are on the stack: an event handler, a timer, an
    /// IPC verb, a D-Bus call. While one runs, a signal write or a property
    /// write marks the graph dirty and nothing more; the one flush happens
    /// when the outermost handler returns. Three writes in a handler used to
    /// be three full flushes, and a bare property write was none at all.
    pub(crate) handler_depth: u32,
    /// Whether something wrote while a handler was running.
    pub(crate) flush_pending: bool,
    pub(crate) logs: morf_runtime::log::Log,
    /// Shaders the configuration registered, by name.
    ///
    /// Compiled once at load. The renderer is handed the generated WGSL when
    /// the host starts up, and a node only ever carries the program's hash.
    pub(crate) shaders: HashMap<String, RegisteredShader>,
    /// The session lock, the primary duty and the output list's revision.
    pub(crate) session: morf_runtime::session::Session,
    /// `morf.clock` and its coarser grains.
    pub(crate) clocks: morf_runtime::wake::Clocks,
    pub(crate) scene: Scene,
    /// Every text input's editing, and which has the keyboard
    /// (`morf_runtime::editing`).
    pub(crate) editing: morf_runtime::editing::Editing,
    /// Which node of each surface has focus (`api_focus.rs`).
    pub(crate) focus: crate::api_focus::FocusState,
    /// Each node's `shortcuts` (`shortcut.rs`), and a sequence half typed.
    pub(crate) shortcuts: HashMap<NodeHandle, crate::shortcut::NodeShortcuts>,
    pub(crate) shortcut_pending: crate::shortcut::Pending,
    /// A modifier pressed on its own, while nothing else has been: a tap of
    /// it if it is let go next.
    pub(crate) modifier_tap: Option<u32>,
    /// Presses under way that may become gestures (`gestures.rs`).
    pub(crate) gestures: crate::gestures::GestureState,
    /// Each surface's overlay layer and what is open on it (`api_overlay.rs`).
    pub(crate) overlays: crate::api_overlay::OverlayState,
    pub(crate) effect_runs: u64,
    /// Each node's handler for each event, and the `contains_pointer` watches.
    pub(crate) events: morf_runtime::events::Events,
    pub(crate) states: HashMap<NodeHandle, StateSet>,
    pub(crate) ipc_handlers: HashMap<String, Handler>,
    /// Captures to be written to a file rather than handed to Lua, by request.
    pub(crate) screencopy_saves: HashMap<u64, crate::image_jobs::CaptureSave>,
    /// `morf.image` work on its way through the worker pool.
    pub(crate) image_jobs: crate::image_jobs::ImageJobs,
    pub(crate) views: morf_runtime::views::Views,
    pub(crate) pam_tasks: Vec<PendingPam>,
    pub(crate) pam_sessions: Vec<PendingPamSession>,
    pub(crate) greetd_sessions: Vec<PendingGreetdSession>,
    /// Every `morf.timer` and `Timer` node, and the clock they run on.
    pub(crate) timers: morf_runtime::timers::Timers,
    pub(crate) timer_callbacks: HashMap<NodeHandle, Handler>,
    /// Where each `ui.Timer` was built, for `MORF_WAKE_LOG`.
    pub(crate) timer_origins: HashMap<NodeHandle, std::rc::Rc<str>>,
    /// Motion beside the scene's own: `on_finished` handlers, loops,
    /// follows, theme fades and exits.
    pub(crate) animation: morf_runtime::animation::Animation,
    /// Each node's `on_destroyed`, until the node goes.
    pub(crate) destroy_hooks: HashMap<NodeHandle, Handler>,
    /// Hooks of nodes already removed, waiting for a moment Lua can run:
    /// removal happens with the state borrowed, often inside a flush.
    pub(crate) pending_destroyed: Vec<Handler>,
    /// The pending hooks are being run; removals they cause join the queue.
    pub(crate) running_destroyed: bool,
    /// Loaders' items and retained nodes, and the handlers told as they go.
    pub(crate) retained: morf_runtime::retention::Retained,
    /// The `measure` and `place` functions of every `ui.Layout` container.
    pub(crate) custom_layouts: HashMap<NodeHandle, CustomLayoutFns>,
    /// The list model's metatable, kept so a list inside `morf.state` is
    /// the same kind of object as `morf.list_model` makes.
    pub(crate) model_metatable: Option<luna::StashedTable>,
    pub(crate) transform_tracker: TransformTracker,
    /// The one metatable every scene-node handle shares.
    ///
    /// Built on first use rather than at install time, because it needs the
    /// arena. Every node used to get its own — a fresh table and two fresh
    /// closures per node, neither of which captured anything node-specific, so
    /// a thousand-node tree allocated three thousand objects that were all the
    /// same. Every other userdata type in this crate already shares one.
    pub(crate) node_metatable: Option<StashedTable>,
    pub(crate) transform_watchers: HashMap<u64, LuaTransformWatcher>,
    pub(crate) next_transform_watcher: u64,
    pub(crate) dbus_signals: Vec<PendingDbusSignal>,
    /// Hands out the ids subscription handles close by.
    pub(crate) next_dbus_signal_id: u64,
    /// `call_async` calls waiting for their answer.
    pub(crate) dbus_replies: Vec<PendingDbusReply>,
    /// The metatable every state proxy shares, so a theme can be one.
    pub(crate) state_metatable: Option<StashedTable>,
    /// Theme token files being watched.
    pub(crate) theme_sources: Vec<ThemeSource>,
    /// `morf.terminal.listen`: terminals of our own hearing colour sequences.
    pub(crate) palette_listeners: Vec<crate::api_palette::PaletteListener>,
    pub(crate) next_palette_listener: u64,
    /// `morf.prefers` and where its answers come from.
    pub(crate) prefers: Option<Prefers>,
    /// `morf.audio`, installed with the runtime and started on first use.
    pub(crate) audio: Option<crate::api_audio::AudioHost>,
    /// `morf.toplevels`, installed with the runtime and fed by
    /// `Runtime::set_windows`.
    pub(crate) toplevels: Option<crate::api_toplevels::ToplevelHost>,
    /// Every bus name `morf.dbus.serve` took, so a runtime that ends or
    /// hands its duties over gives them back first.
    pub(crate) owned_bus_names: Vec<std::rc::Weak<std::cell::RefCell<morf_io::DbusService>>>,
    pub(crate) dbus_services: morf_io::DbusHandlers<morf_runtime::Handler>,
    pub(crate) udev_monitors: Vec<PendingUdev>,
    pub(crate) status_notifiers: Vec<PendingStatusNotifier>,
    /// `morf.http` requests in flight.
    pub(crate) http_requests: Vec<crate::api_http::PendingHttp>,
    /// `morf.spawn` children and `morf.connect` connections.
    pub(crate) io: crate::api_io::IoHub,
    /// `morf.fs.watch` watches.
    pub(crate) watches: crate::api_watch::WatchHub,
    /// `ui.Terminal` nodes: their emulators and their programs.
    pub(crate) terminals: crate::terminals::TerminalHub,
    /// Text nodes set in runs, whose links the layout places.
    pub(crate) linked_texts: std::collections::HashSet<NodeHandle>,
    /// Every `ui.Image`: what became of its source, and its playback.
    pub(crate) images: crate::images::ImageNodes,
    pub(crate) shell_root: PathBuf,
    /// What handlers asked of the platform, queued for the host.
    pub(crate) requests: morf_runtime::requests::Requests,
    /// The windows the configuration declared, as the host reads them.
    pub(crate) windows: morf_runtime::windows::Declarations,
}
