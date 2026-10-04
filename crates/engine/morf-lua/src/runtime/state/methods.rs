//! `ReactiveState`'s methods: a fresh state and the graph's garbage.

use super::*;

impl ReactiveState {
    pub(crate) fn new() -> Self {
        Self {
            engine: morf_runtime::Engine::new(),
            limits: crate::Limits::default(),
            channels: crate::channels::Channels::default(),
            shaders: HashMap::new(),
            overlays: Default::default(),
            screencopy_saves: HashMap::new(),
            image_jobs: crate::image_jobs::ImageJobs::default(),
            pam_tasks: Vec::new(),
            pam_sessions: Vec::new(),
            greetd_sessions: Vec::new(),
            images: Default::default(),
            model_metatable: None,
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
        }
    }
}
