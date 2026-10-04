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
        let at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_millis() as u64)
            .unwrap_or(0);
        // A cap, oldest out first: a shell that logs a warning a second
        // would otherwise hold a day of them.
        if self.logs.len() >= MAX_LOG_ENTRIES {
            let excess = self.logs.len() + 1 - MAX_LOG_ENTRIES;
            self.logs.drain(..excess);
        }
        let mut message = message.into();
        if message.len() > MAX_LOG_MESSAGE {
            let mut cut = MAX_LOG_MESSAGE;
            while !message.is_char_boundary(cut) {
                cut -= 1;
            }
            message.truncate(cut);
            message.push('…');
        }
        self.logs.push(LogEntry {
            level,
            at_ms,
            message,
        });
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
            limits: crate::Limits::default(),
            reactive: morf_runtime::reactive::Reactive {
                values,
                signals: vec![clock, clock_minutes, clock_hours, session_lock],
                ..morf_runtime::reactive::Reactive::new(graph)
            },
            property_signals: HashMap::new(),
            current_property_names: HashMap::new(),
            property_revision: 0,
            model_revisions: HashMap::new(),
            scene_revision: 0,
            polled_revision: 0,
            service_definitions_revision: u64::MAX,
            hidden_revisions: 0,
            shared: crate::shared::SharedValues::default(),
            channels: crate::channels::Channels::default(),
            reload_seed: HashMap::new(),
            reloadable: HashMap::new(),
            reload_request: None,
            watch_files: true,
            watch_files_changed: false,
            quit_requested: false,
            workspace_requests: Vec::new(),
            toplevel_requests: Vec::new(),
            idle_inhibited: false,
            idle_inhibit_changed: false,
            shortcuts_inhibited: false,
            shortcuts_inhibit_changed: false,
            shortcuts_callbacks: Vec::new(),
            lint_warned: HashSet::new(),
            capabilities: Vec::new(),
            reload_completed_callbacks: Vec::new(),
            reload_failed_callbacks: Vec::new(),
            next_effect: 0,
            active: None,
            handler_depth: 0,
            flush_pending: false,
            logs: Vec::new(),
            shaders: HashMap::new(),
            scene: Scene::new(),
            editing: Default::default(),
            focus: Default::default(),
            shortcuts: HashMap::new(),
            shortcut_pending: Default::default(),
            modifier_tap: None,
            gestures: Default::default(),
            overlays: Default::default(),
            clipboard_text: None,
            effect_runs: 0,
            clock,
            clock_minutes,
            clock_hours,
            session_lock,
            session_lock_callbacks: Vec::new(),
            lock_surface_builder: None,
            events: morf_runtime::events::Events::default(),
            parent_transitions: Vec::new(),
            states: HashMap::new(),
            ipc_handlers: HashMap::new(),
            idle_callbacks: HashMap::new(),
            next_idle_subscription: 0,
            idle_timeouts_changed: false,
            output_power_requests: Vec::new(),
            gamma_requests: Vec::new(),
            clipboard_requests: Vec::new(),
            clipboard_callbacks: Vec::new(),
            clipboard_watchers: Vec::new(),
            offer_reads: Vec::new(),
            offer_read_callbacks: HashMap::new(),
            next_offer_read: 0,
            drag_requests: Vec::new(),
            drag_end_callbacks: Vec::new(),
            keyboard_focus_callbacks: Vec::new(),
            backdrop_callbacks: Vec::new(),
            screencopy_requests: Vec::new(),
            screencopy_callbacks: HashMap::new(),
            screencopy_names: HashMap::new(),
            screencopy_releases: Vec::new(),
            screencopy_saves: HashMap::new(),
            next_screencopy: 0,
            image_jobs: crate::image_jobs::ImageJobs::default(),
            virtual_keyboard_requests: Vec::new(),
            input_method_enable_requested: false,
            input_method_requests: Vec::new(),
            input_method_callbacks: Vec::new(),
            text_input_enable_requested: false,
            text_input_requests: Vec::new(),
            text_input_callbacks: Vec::new(),
            views: HashMap::new(),
            pam_tasks: Vec::new(),
            pam_sessions: Vec::new(),
            greetd_sessions: Vec::new(),
            timers: morf_runtime::timers::Timers::default(),
            timer_callbacks: HashMap::new(),
            timer_origins: HashMap::new(),
            destroy_hooks: HashMap::new(),
            linked_texts: Default::default(),
            follows: Vec::new(),
            images: Default::default(),
            node_loops: HashMap::new(),
            pending_destroyed: Vec::new(),
            running_destroyed: false,
            animation_callbacks: HashMap::new(),
            group_callbacks: HashMap::new(),
            loader_factories: HashMap::new(),
            failed_loaders: HashSet::new(),
            custom_layouts: HashMap::new(),
            model_metatable: None,
            loaded_loaders: HashSet::new(),
            dormant_loaders: HashSet::new(),
            preload_pending: HashMap::new(),
            retention: Retention::default(),
            retain_callbacks: HashMap::new(),
            retained_destroy_queue: HashSet::new(),
            exit_registered: HashSet::new(),
            window_surfaces: HashMap::new(),
            surface_handlers: HashMap::new(),
            next_window_surface: 0,
            window_surfaces_changed: false,
            window_sizes: HashMap::new(),
            window_handlers: HashMap::new(),
            layer_surface_changed: false,
            window_surface_actions: Vec::new(),
            popup_node_anchors: HashMap::new(),
            transform_tracker: TransformTracker::default(),
            node_metatable: None,
            transform_watchers: HashMap::new(),
            next_transform_watcher: 0,
            state_metatable: None,
            theme_sources: Vec::new(),
            theme_fades: Vec::new(),
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
            session_unlock_requested: false,
            layer_surface: LayerSurfaceConfig::default(),
            shell_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }
}
