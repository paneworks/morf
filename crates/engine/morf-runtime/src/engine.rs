//! The engine: every subsystem a configuration's handlers are called from,
//! each owning its own state, in one struct.
//!
//! The scripting layer holds one beside what only it can hold (its stashed
//! tables, its handler store, the hubs of the platform crates this one may
//! not name) and calls into the subsystems through it; the subsystems call
//! back through [`crate::Handlers`] or a host trait of their own
//! ([`crate::views::ViewHost`], [`crate::editing::EditHost`], ...).

use std::collections::HashMap;
use std::path::Path;

use morf_layout::TransformTracker;
use morf_scene::Scene;
use morf_scene::reactive::{Graph, SignalId};
use morf_value::IpcValue;

mod access;
mod bridges;
mod queries;
mod scene;
mod services;
pub use queries::{PRELOAD_PATIENCE, node_path};
pub use scene::{scoped_id, validate_scope_part};

pub struct Engine {
    /// The graph, the signal mirror, the declared effects and the flush.
    pub reactive: crate::reactive::Reactive,
    /// The handlers on the stack, the effect being captured, and the signals
    /// standing for node properties.
    pub tracking: crate::tracking::Tracking,
    /// How far the scene has moved, and how far each reader of it has seen.
    pub revisions: crate::revisions::Revisions,
    /// Each list model a binding has read, by address, with the revision
    /// signal that binding depends on.
    model_revisions: crate::models::ModelRevisions,
    /// `morf.shared` values: this copy's signals for them, and what to publish.
    shared: crate::shared::SharedValues,
    /// A reload, file watching, quitting and unlocking, as asked, and the
    /// values kept across a reload.
    pub lifecycle: crate::requests::Lifecycle,
    /// What this output can do, and how big one of morf's pixels is on it.
    pub display: crate::display::Display,
    /// The shell's log lines.
    logs: crate::log::Log,
    /// The session lock, the primary duty and the output list's revision.
    pub session: crate::session::Session,
    /// `morf.clock` and its coarser grains.
    clocks: crate::wake::Clocks,
    pub scene: Scene,
    /// Every text input's editing, and which has the keyboard.
    pub editing: crate::editing::Editing,
    /// Which node of each surface has focus.
    pub focus: crate::focus::FocusState,
    /// Each node's `shortcuts`, a sequence half typed, a modifier's tap.
    shortcuts:crate::shortcuts::Shortcuts,
    /// Presses under way that may become gestures.
    pub gestures: crate::gestures::Gestures,
    /// Each node's handler for each event, and the `contains_pointer` watches.
    pub events: crate::events::Events,
    /// Each node's declared states.
    states: crate::states::States,
    /// The verbs answered over IPC.
    ipc: crate::ipc::IpcVerbs,
    pub views: crate::views::Views,
    /// Every timer and `Timer` node, and the clock they run on.
    pub timers: crate::timers::Timers,
    /// Motion beside the scene's own: `on_finished` handlers, loops,
    /// follows, theme fades and exits.
    pub animation: crate::animation::Animation,
    /// Each node's `on_destroyed`, and those of removed nodes waiting to run.
    destroy_hooks: crate::destroy::DestroyHooks,
    /// Loaders' items and retained nodes, and the handlers told as they go.
    pub retained: crate::retention::Retained,
    /// Each `ui.Layout` container's functions, and the texts whose links
    /// the layout places.
    layout_hooks: crate::layout::LayoutHooks,
    /// Where each node was laid out, and how it is transformed.
    transform_tracker: TransformTracker,
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
        let mut reactive = crate::reactive::Reactive::new(graph);
        for (signal, value) in clock_values {
            reactive.adopt(signal, value);
        }
        reactive.adopt(session_lock, initial_lock);
        Self {
            reactive,
            tracking: Default::default(),
            revisions: Default::default(),
            model_revisions: Default::default(),
            shared: Default::default(),
            lifecycle: Default::default(),
            display: Default::default(),
            logs: Default::default(),
            session: crate::session::Session::new(session_lock),
            clocks,
            scene: Scene::new(),
            editing: Default::default(),
            focus: Default::default(),
            shortcuts: Default::default(),
            gestures: Default::default(),
            events: Default::default(),
            states: Default::default(),
            ipc: Default::default(),
            views: HashMap::new(),
            timers: Default::default(),
            animation: Default::default(),
            destroy_hooks: Default::default(),
            retained: Default::default(),
            layout_hooks: Default::default(),
            transform_tracker: TransformTracker::default(),
            requests: Default::default(),
            windows: Default::default(),
        }
    }

    /// The clock signal a reader at `precision` depends on.
    pub fn clock_signal(&self, precision: crate::wake::ClockPrecision) -> SignalId {
        self.clocks.signal(precision)
    }

    /// Where the configuration's own files are.
    pub fn shell_root(&self) -> &Path {
        self.lifecycle.shell_root()
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
        assert_eq!(engine.reactive.signal_count(), 4);
        assert!(engine.reactive.value(engine.session.lock).is_some());
        assert!(engine.scene.roots().is_empty());
        assert!(!engine.tracking.in_handler());
        assert!(!engine.tracking.flush_pending());
    }

    #[test]
    fn the_log_takes_lines_with_their_level() {
        let mut engine = Engine::new();
        engine.log(crate::log::LogLevel::Warn, "careful");
        assert_eq!(engine.logs.len(), 1);
    }
}
