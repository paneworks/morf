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
            theme_sources: Default::default(),
            palettes: Default::default(),
            prefers: None,
            audio: None,
            toplevels: None,
            dbus: Default::default(),
            feeds: Default::default(),
            io: Default::default(),
            watches: Default::default(),
            terminals: Default::default(),
            loading_unapproved: Vec::new(),
        }
    }

    pub(crate) fn begin_loading_unapproved(&mut self, module: String) {
        self.loading_unapproved.push(module);
    }

    pub(crate) fn end_loading_unapproved(&mut self) {
        self.loading_unapproved.pop();
    }

    /// The refusal a gated API returns while an unapproved imported
    /// module's top level runs: `permission: module `x` is not approved
    /// to use <group> (run `morf approve`)`.
    pub(crate) fn permission_refusal(&self, group: &str) -> Option<String> {
        self.loading_unapproved.last().map(|module| {
            format!(
                "permission: module `{module}` is not approved to use {group} \
                 at load time (run `morf approve`)"
            )
        })
    }
}
