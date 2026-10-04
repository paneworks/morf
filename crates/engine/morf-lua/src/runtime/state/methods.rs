//! `ReactiveState`'s methods: a fresh state, the log, the clock signals,
//! and the graph's garbage.

use super::*;

impl ReactiveState {
    /// Hands the graph what removed nodes left behind, when it is here to
    /// take them; while a flush holds it they wait for the next call.
    pub(crate) fn collect_graph_garbage(&mut self) {
        if self.reactive.flushing {
            return;
        }
        self.collect_dead_models();
        self.reactive.collect_garbage();
    }

    /// Hands Lua effect `token` to the graph, or queues it while a flush
    /// holds the graph. Either way the effect runs on the next flush.
    /// Building a node with bindings inside an effect used to panic the
    /// whole engine here.
    pub(crate) fn register_external_effect(&mut self, token: u64, name: String) {
        self.reactive.register_external_effect(token, name);
    }

    /// The clock signal a reader at `precision` depends on.
    pub(crate) fn clock_signal(&self, precision: crate::ClockPrecision) -> SignalId {
        match precision {
            crate::ClockPrecision::Seconds => self.clock,
            crate::ClockPrecision::Minutes => self.clock_minutes,
            crate::ClockPrecision::Hours => self.clock_hours,
        }
    }

    /// Records one line, stamped with when it happened.
    ///
    /// The one way in, so every entry gets a level and a time rather than the
    /// flat strings this used to hold -- a shell running for a day accumulates
    /// thousands, and without either there is no way to ask which are serious
    /// or recent.
    pub(crate) fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        self.logs.push(level, message);
    }

    pub(crate) fn new() -> Self {
        let mut graph = Graph::default();
        let initial_clock = IpcValue::String(String::new());
        let clock = graph.signal("morf.clock", initial_clock.clone());
        let clock_minutes = graph.signal("morf.minute_clock", initial_clock.clone());
        let clock_hours = graph.signal("morf.hour_clock", initial_clock.clone());
        let initial_lock = IpcValue::String(crate::SessionLockState::Unlocked.name().to_owned());
        let session_lock = graph.signal("morf.session_lock", initial_lock.clone());
        let mut values = HashMap::new();
        values.insert(clock, initial_clock.clone());
        values.insert(clock_minutes, initial_clock.clone());
        values.insert(clock_hours, initial_clock);
        values.insert(session_lock, initial_lock);
        Self {
            requests: Default::default(),
            windows: Default::default(),
            limits: crate::Limits::default(),
            reactive: morf_runtime::reactive::Reactive {
                values,
                signals: vec![clock, clock_minutes, clock_hours, session_lock],
                ..morf_runtime::reactive::Reactive::new(graph)
            },
            property_signals: HashMap::new(),
            current_property_names: HashMap::new(),
            revisions: Default::default(),
            model_revisions: Default::default(),
            shared: crate::shared::SharedValues::default(),
            channels: crate::channels::Channels::default(),
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
            shaders: HashMap::new(),
            scene: Scene::new(),
            editing: Default::default(),
            focus: Default::default(),
            shortcuts: HashMap::new(),
            shortcut_pending: Default::default(),
            modifier_tap: None,
            gestures: Default::default(),
            overlays: Default::default(),
            effect_runs: 0,
            clock,
            clock_minutes,
            clock_hours,
            session_lock,
            session_lock_callbacks: Vec::new(),
            lock_surface_builder: None,
            events: morf_runtime::events::Events::default(),
            states: HashMap::new(),
            ipc_handlers: HashMap::new(),
            screencopy_saves: HashMap::new(),
            image_jobs: crate::image_jobs::ImageJobs::default(),
            views: HashMap::new(),
            pam_tasks: Vec::new(),
            pam_sessions: Vec::new(),
            greetd_sessions: Vec::new(),
            timers: morf_runtime::timers::Timers::default(),
            timer_callbacks: HashMap::new(),
            timer_origins: HashMap::new(),
            destroy_hooks: HashMap::new(),
            linked_texts: Default::default(),
            images: Default::default(),
            pending_destroyed: Vec::new(),
            running_destroyed: false,
            retained: Default::default(),
            custom_layouts: HashMap::new(),
            model_metatable: None,
            animation: morf_runtime::animation::Animation::default(),
            transform_tracker: TransformTracker::default(),
            node_metatable: None,
            transform_watchers: HashMap::new(),
            next_transform_watcher: 0,
            state_metatable: None,
            theme_sources: Vec::new(),
            palette_listeners: Vec::new(),
            next_palette_listener: 0,
            prefers: None,
            audio: None,
            toplevels: None,
            screens_revision: None,
            screens_signature: String::new(),
            primary: None,
            primary_callbacks: Vec::new(),
            owned_bus_names: Vec::new(),
            dbus_signals: Vec::new(),
            next_dbus_signal_id: 0,
            dbus_replies: Vec::new(),
            dbus_services: Default::default(),
            udev_monitors: Vec::new(),
            status_notifiers: Vec::new(),
            http_requests: Vec::new(),
            io: Default::default(),
            watches: Default::default(),
            terminals: Default::default(),
            shell_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }
}
