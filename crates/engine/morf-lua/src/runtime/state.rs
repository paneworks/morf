//! `ReactiveState`, everything one runtime holds between turns, and the
//! types its fields are made of.

pub(crate) use crate::api::media::shader::RegisteredShader;

use morf_runtime::Handler;
use morf_scene::reactive::SignalId;
use morf_scene::{ListModel, NodeHandle, Scene};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// Re-exported, because these moved out of this file only to satisfy the line
// gate: every consumer reaches for them through `state::*` and there is no
// reason to make them all learn a second module name.
pub(crate) use crate::runtime::pending::*;
pub(crate) use crate::value::tokens::*;
pub(crate) use crate::vm::state_cell::{StateCell, StateRef, StateRefMut};

mod access;
mod auth;
mod bridges;
mod claims;
mod dbus;
mod feeds;
mod follow;
mod metatables;
mod methods;

pub(crate) use auth::Auth;
pub(crate) use dbus::DbusState;
pub(crate) use feeds::Feeds;
pub(crate) use metatables::Metatables;

pub(crate) use claims::{ClaimsPass, run_claims};
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

pub(crate) use morf_runtime::layout::CustomLayoutFns;

#[derive(Clone, Copy)]
pub(crate) enum ViewKind {
    Repeater,
    List,
    Grid,
}

pub(crate) use morf_runtime::reactive::Effect as LuaEffect;

pub(crate) use morf_runtime::retention::RetainCallbacks;

pub(crate) use morf_runtime::reactive::{ClaimInput, ClaimSink, EffectSink, PropertySink};

#[cfg(test)]
pub(crate) use morf_runtime::log::MAX_LOG_ENTRIES;

pub(crate) struct ReactiveState {
    /// Every subsystem the engine runs (`morf_runtime::Engine`); what
    /// follows is what only the Lua layer can hold.
    engine: morf_runtime::Engine,
    /// The runtime's limits, for a binding made where they are not at hand
    /// (a function assigned to a node's property).
    limits: crate::Limits,
    /// The channels the configuration made, and the nodes drawing them.
    channels: crate::api::ui::channels::Channels,
    /// Shaders the configuration registered, by name.
    ///
    /// Compiled once at load. The renderer is handed the generated WGSL when
    /// the host starts up, and a node only ever carries the program's hash.
    shaders: HashMap<String, RegisteredShader>,
    /// Each surface's overlay layer and what is open on it (`api_overlay.rs`).
    overlays: crate::api::input::overlay::OverlayState,
    /// Captures to be written to a file rather than handed to Lua, by request.
    capture_saves: crate::runtime::image_jobs::CaptureSaves,
    /// `morf.image` work on its way through the worker pool.
    image_jobs: crate::runtime::image_jobs::ImageJobs,
    /// `morf.pam` checks and conversations, and greetd logins.
    auth: Auth,
    /// The metatables list models, scene nodes and state proxies share.
    metatables: Metatables,
    /// `ui.watch_transform` watches.
    transform_watchers: morf_runtime::layout::TransformWatches,
    /// What extensions want told when nodes are removed.
    removal_listeners: Vec<crate::vm::extensions::NodesRemoved>,
    /// `morf.dbus`: subscriptions, calls waiting on an answer, and the
    /// names and services this runtime serves.
    dbus: DbusState,
    /// Theme token files being watched.
    theme_sources: ThemeSources,
    /// `morf.terminal.listen`: terminals of our own hearing colour sequences.
    palettes: crate::api::media::terminal_palette::PaletteListeners,
    /// `morf.prefers` and where its answers come from.
    prefers: Option<Prefers>,
    /// `morf.audio`, installed with the runtime and started on first use.
    audio: Option<crate::api::media::audio::AudioHost>,
    /// `morf.toplevels`, installed with the runtime and fed by
    /// `Runtime::set_windows`.
    toplevels: Option<crate::api::desktop::toplevels::ToplevelHost>,
    /// udev monitors, tray hosts and `morf.http` requests in flight.
    feeds: Feeds,
    /// `morf.spawn` children and `morf.connect` connections.
    io: crate::api::io::io::IoHub,
    /// `morf.fs.watch` watches.
    watches: crate::api::io::watch::WatchHub,
    /// `ui.Terminal` nodes: their emulators and their programs.
    terminals: crate::runtime::terminals::TerminalHub,
    /// Every `ui.Image`: what became of its source, and its playback.
    images: crate::runtime::images::ImageNodes,
    /// Unapproved imported modules whose top level is running now
    /// (PLAN item 4 step 3, load-time enforcement): gated APIs refuse
    /// while this is non-empty.
    loading_unapproved: Vec<String>,
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
