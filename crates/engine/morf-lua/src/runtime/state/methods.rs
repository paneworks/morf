//! `ReactiveState`'s methods: a fresh state and the graph's garbage.

use super::*;

impl ReactiveState {
    pub(crate) fn new() -> Self {
        Self {
            engine: morf_runtime::Engine::new(),
            limits: crate::Limits::default(),
            channels: crate::api::ui::channels::Channels::default(),
            shaders: HashMap::new(),
            overlays: Default::default(),
            capture_saves: Default::default(),
            image_jobs: crate::runtime::image_jobs::ImageJobs::default(),
            auth: Default::default(),
            images: Default::default(),
            metatables: Default::default(),
            transform_watchers: Default::default(),
            removal_listeners: Vec::new(),
            theme_sources: Vec::new(),
            palettes: Default::default(),
            prefers: None,
            audio: None,
            toplevels: None,
            dbus: Default::default(),
            udev_monitors: Vec::new(),
            status_notifiers: Vec::new(),
            http_requests: Vec::new(),
            io: Default::default(),
            watches: Default::default(),
            terminals: Default::default(),
        }
    }
}
