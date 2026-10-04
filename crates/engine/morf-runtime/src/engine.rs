//! The engine: every subsystem a configuration's handlers are called from,
//! each owning its own state, in one struct.
//!
//! The scripting layer holds one beside what only it can hold (its stashed
//! tables, its handler store, the hubs of the platform crates this one may
//! not name) and calls into the subsystems through it; the subsystems call
//! back through [`crate::Handlers`] or a host trait of their own
//! ([`crate::views::ViewHost`], [`crate::editing::EditHost`], ...).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use morf_layout::TransformTracker;
use morf_scene::reactive::{Graph, SignalId};
use morf_scene::{NodeHandle, Scene};
use morf_value::IpcValue;

use crate::Handler;

mod queries;
mod scene;
mod services;
pub use queries::{PRELOAD_PATIENCE, node_path};
pub use scene::{scoped_id, validate_scope_part};

/// A `ui.Layout` container's `measure` and `place` functions.
#[derive(Clone)]
pub struct CustomLayoutFns {
    pub measure: Handler,
    pub place: Handler,
}

pub struct Engine {
    /// The graph, the signal mirror, the declared effects and the flush.
    pub reactive: crate::reactive::Reactive,
    pub property_signals: HashMap<(NodeHandle, String, bool), SignalId>,
    pub current_property_names: HashMap<String, (NodeHandle, String)>,
    /// How far the scene has moved, and how far each reader of it has seen.
    pub revisions: crate::reactive::Revisions,
    /// Each list model a binding has read, by address, with the revision
    /// signal that binding depends on.
    pub model_revisions: crate::models::ModelRevisions,
    /// `morf.shared` values: this copy's signals for them, and what to publish.
    pub shared: crate::shared::SharedValues,
    pub reload_seed: HashMap<String, IpcValue>,
    pub reloadable: HashMap<String, SignalId>,
    /// A reload, file watching, quitting and unlocking, as asked.
    pub lifecycle: crate::requests::Lifecycle,
    /// Nodes the lint has already complained about, so a bar that paints
    /// sixty times a second says it once.
    pub lint_warned: HashSet<NodeHandle>,
    /// What this output's compositor and GPU can do, as name = value pairs.
    pub capabilities: Vec<(String, String)>,
    pub next_effect: u64,
    pub active: Option<crate::states::Capture>,
    /// How many handlers are on the stack. While one runs, a write marks the
    /// graph dirty and nothing more; the one flush happens when the outermost
    /// handler returns.
    pub handler_depth: u32,
    /// Whether something wrote while a handler was running.
    pub flush_pending: bool,
    pub logs: crate::log::Log,
    /// The session lock, the primary duty and the output list's revision.
    pub session: crate::session::Session,
    /// `morf.clock` and its coarser grains.
    pub clocks: crate::wake::Clocks,
    pub scene: Scene,
    /// Every text input's editing, and which has the keyboard.
    pub editing: crate::editing::Editing,
    /// Which node of each surface has focus.
    pub focus: crate::focus::FocusState,
    /// Each node's `shortcuts`, and a sequence half typed.
    pub shortcuts: HashMap<NodeHandle, crate::shortcuts::NodeShortcuts>,
    pub shortcut_pending: crate::shortcuts::Pending,
    /// A modifier pressed on its own, while nothing else has been: a tap of
    /// it if it is let go next.
    pub modifier_tap: Option<u32>,
    /// Presses under way that may become gestures.
    pub gestures: crate::gestures::Gestures,
    pub effect_runs: u64,
    /// Each node's handler for each event, and the `contains_pointer` watches.
    pub events: crate::events::Events,
    pub states: HashMap<NodeHandle, crate::states::StateSet>,
    pub ipc_handlers: HashMap<String, Handler>,
    pub views: crate::views::Views,
    /// Every timer and `Timer` node, and the clock they run on.
    pub timers: crate::timers::Timers,
    pub timer_callbacks: HashMap<NodeHandle, Handler>,
    /// Where each `ui.Timer` was built, for `MORF_WAKE_LOG`.
    pub timer_origins: HashMap<NodeHandle, Rc<str>>,
    /// Motion beside the scene's own: `on_finished` handlers, loops,
    /// follows, theme fades and exits.
    pub animation: crate::animation::Animation,
    /// Each node's `on_destroyed`, until the node goes.
    pub destroy_hooks: HashMap<NodeHandle, Handler>,
    /// Hooks of nodes already removed, waiting for a moment handlers can run:
    /// removal happens with the state borrowed, often inside a flush.
    pub pending_destroyed: Vec<Handler>,
    /// The pending hooks are being run; removals they cause join the queue.
    pub running_destroyed: bool,
    /// Loaders' items and retained nodes, and the handlers told as they go.
    pub retained: crate::retention::Retained,
    /// The `measure` and `place` functions of every `ui.Layout` container.
    pub custom_layouts: HashMap<NodeHandle, CustomLayoutFns>,
    pub transform_tracker: TransformTracker,
    /// Text nodes set in runs, whose links the layout places.
    pub linked_texts: HashSet<NodeHandle>,
    /// Where the configuration's own files are.
    pub shell_root: PathBuf,
    /// What handlers asked of the platform, queued for the host.
    pub requests: crate::requests::Requests,
    /// The windows the configuration declared, as the host reads them.
    pub windows: crate::windows::Declarations,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// An engine with an empty scene, its clocks and its session-lock signal
    /// in the graph.
    pub fn new() -> Self {
        let mut graph = Graph::default();
        let (clocks, clock_values) = crate::wake::Clocks::new(&mut graph);
        let initial_lock =
            IpcValue::String(crate::session::SessionLockState::Unlocked.name().to_owned());
        let session_lock = graph.signal("morf.session_lock", initial_lock.clone());
        let mut values: HashMap<_, _> = clock_values.into_iter().collect();
        values.insert(session_lock, initial_lock);
        Self {
            reactive: crate::reactive::Reactive {
                values,
                signals: vec![clocks.seconds, clocks.minutes, clocks.hours, session_lock],
                ..crate::reactive::Reactive::new(graph)
            },
            property_signals: HashMap::new(),
            current_property_names: HashMap::new(),
            revisions: Default::default(),
            model_revisions: Default::default(),
            shared: Default::default(),
            reload_seed: HashMap::new(),
            reloadable: HashMap::new(),
            lifecycle: Default::default(),
            lint_warned: HashSet::new(),
            capabilities: Vec::new(),
            next_effect: 0,
            active: None,
            handler_depth: 0,
            flush_pending: false,
            logs: Default::default(),
            session: crate::session::Session::new(session_lock),
            clocks,
            scene: Scene::new(),
            editing: Default::default(),
            focus: Default::default(),
            shortcuts: HashMap::new(),
            shortcut_pending: Default::default(),
            modifier_tap: None,
            gestures: Default::default(),
            effect_runs: 0,
            events: Default::default(),
            states: HashMap::new(),
            ipc_handlers: HashMap::new(),
            views: HashMap::new(),
            timers: Default::default(),
            timer_callbacks: HashMap::new(),
            timer_origins: HashMap::new(),
            animation: Default::default(),
            destroy_hooks: HashMap::new(),
            pending_destroyed: Vec::new(),
            running_destroyed: false,
            retained: Default::default(),
            custom_layouts: HashMap::new(),
            transform_tracker: TransformTracker::default(),
            linked_texts: HashSet::new(),
            shell_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            requests: Default::default(),
            windows: Default::default(),
        }
    }

    /// The clock signal a reader at `precision` depends on.
    pub fn clock_signal(&self, precision: crate::wake::ClockPrecision) -> SignalId {
        self.clocks.signal(precision)
    }

    /// Records one log line, stamped with when it happened.
    pub fn log(&mut self, level: crate::log::LogLevel, message: impl Into<String>) {
        self.logs.push(level, message);
    }

    /// Hands Lua effect `token` to the graph, or queues it while a flush
    /// holds the graph. Either way the effect runs on the next flush.
    pub fn register_external_effect(&mut self, token: u64, name: String) {
        self.reactive.register_external_effect(token, name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_engine_has_its_clocks_and_lock_in_the_graph() {
        let engine = Engine::new();
        assert_eq!(engine.reactive.signals.len(), 4);
        assert!(engine.reactive.values.contains_key(&engine.session.lock));
        assert!(engine.scene.roots().is_empty());
        assert_eq!(engine.handler_depth, 0);
        assert!(!engine.flush_pending);
    }

    #[test]
    fn the_log_takes_lines_with_their_level() {
        let mut engine = Engine::new();
        engine.log(crate::log::LogLevel::Warn, "careful");
        assert_eq!(engine.logs.len(), 1);
    }
}
