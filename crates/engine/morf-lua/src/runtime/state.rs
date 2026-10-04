//! `ReactiveState`, everything one runtime holds between turns, and the
//! types its fields are made of.

pub(crate) use crate::api_shader::RegisteredShader;
use luna::StashedTable;

use morf_runtime::Handler;
use morf_scene::reactive::SignalId;
use morf_scene::{ListModel, NodeHandle, Scene};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

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

pub(crate) use morf_runtime::engine::CustomLayoutFns;

#[derive(Clone, Copy)]
pub(crate) enum ViewKind {
    Repeater,
    List,
    Grid,
}

pub(crate) use morf_runtime::layout::TransformWatch as LuaTransformWatcher;
pub(crate) use morf_runtime::windows::PopupNodeAnchor;

pub(crate) use morf_runtime::reactive::Effect as LuaEffect;

pub(crate) use morf_runtime::retention::RetainCallbacks;

pub(crate) use morf_runtime::reactive::{EffectSink, PropertySink};

#[cfg(test)]
pub(crate) use morf_runtime::log::MAX_LOG_ENTRIES;

pub(crate) struct ReactiveState {
    /// Every subsystem the engine runs (`morf_runtime::Engine`); what
    /// follows is what only the Lua layer can hold.
    pub(crate) engine: morf_runtime::Engine,
    /// The runtime's limits, for a binding made where they are not at hand
    /// (a function assigned to a node's property).
    pub(crate) limits: crate::Limits,
    pub(crate) channels: crate::channels::Channels,
    /// Shaders the configuration registered, by name.
    ///
    /// Compiled once at load. The renderer is handed the generated WGSL when
    /// the host starts up, and a node only ever carries the program's hash.
    pub(crate) shaders: HashMap<String, RegisteredShader>,
    /// Each surface's overlay layer and what is open on it (`api_overlay.rs`).
    pub(crate) overlays: crate::api_overlay::OverlayState,
    /// Captures to be written to a file rather than handed to Lua, by request.
    pub(crate) screencopy_saves: HashMap<u64, crate::image_jobs::CaptureSave>,
    /// `morf.image` work on its way through the worker pool.
    pub(crate) image_jobs: crate::image_jobs::ImageJobs,
    pub(crate) pam_tasks: Vec<PendingPam>,
    pub(crate) pam_sessions: Vec<PendingPamSession>,
    pub(crate) greetd_sessions: Vec<PendingGreetdSession>,
    /// The list model's metatable, kept so a list inside `morf.state` is
    /// the same kind of object as `morf.list_model` makes.
    pub(crate) model_metatable: Option<luna::StashedTable>,
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
    /// Every `ui.Image`: what became of its source, and its playback.
    pub(crate) images: crate::images::ImageNodes,
}

impl std::ops::Deref for ReactiveState {
    type Target = morf_runtime::Engine;

    fn deref(&self) -> &Self::Target {
        &self.engine
    }
}

impl std::ops::DerefMut for ReactiveState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.engine
    }
}
