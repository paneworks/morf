//! Each part of the Lua layer's state, reached by reference.

use super::*;

/// A shared and an exclusive accessor for each field.
macro_rules! parts {
    ($($(#[$doc:meta])* $field:ident, $field_mut:ident: $ty:ty;)*) => {
        impl ReactiveState {
            $(
                $(#[$doc])*
                pub(crate) fn $field(&self) -> &$ty {
                    &self.$field
                }

                $(#[$doc])*
                pub(crate) fn $field_mut(&mut self) -> &mut $ty {
                    &mut self.$field
                }
            )*
        }
    };
}

parts! {
    /// Every subsystem the engine runs.
    engine, engine_mut: morf_runtime::Engine;
    /// The runtime's limits.
    limits, limits_mut: crate::Limits;
    /// The channels the configuration made.
    channels, channels_mut: crate::api::ui::channels::Channels;
    /// Shaders the configuration registered, by name.
    shaders, shaders_mut: HashMap<String, RegisteredShader>;
    /// Each surface's overlay layer and what is open on it.
    overlays, overlays_mut: crate::api::input::overlay::OverlayState;
    /// Captures to be written to a file.
    capture_saves, capture_saves_mut: crate::runtime::image_jobs::CaptureSaves;
    /// `morf.pam` checks and conversations, and greetd logins.
    auth, auth_mut: Auth;
    /// The metatables list models, scene nodes and state proxies share.
    metatables, metatables_mut: Metatables;
    /// `ui.watch_transform` watches.
    transform_watchers, transform_watchers_mut: morf_runtime::layout::TransformWatches;
    /// What extensions want told when nodes are removed.
    removal_listeners, removal_listeners_mut: Vec<crate::vm::extensions::NodesRemoved>;
    /// `morf.dbus`.
    dbus, dbus_mut: DbusState;
    /// Theme token files being watched.
    theme_sources, theme_sources_mut: ThemeSources;
    /// `morf.terminal.listen` terminals.
    palettes, palettes_mut: crate::api::media::terminal_palette::PaletteListeners;
    /// `morf.prefers` and where its answers come from.
    prefers, prefers_mut: Option<Prefers>;
    /// udev monitors, tray hosts and HTTP requests in flight.
    feeds, feeds_mut: Feeds;
    /// `morf.fs.watch` watches.
    watches, watches_mut: crate::api::io::watch::WatchHub;
    /// `ui.Terminal` nodes.
    terminals, terminals_mut: crate::runtime::terminals::TerminalHub;
    /// Every `ui.Image`.
    images, images_mut: crate::runtime::images::ImageNodes;
    /// The load-time gate over imported modules.
    permissions, permissions_mut: crate::vm::permissions::Gate;
}

impl ReactiveState {
    /// `morf.image` work on its way through the worker pool.
    pub(crate) fn image_jobs_mut(&mut self) -> &mut crate::runtime::image_jobs::ImageJobs {
        &mut self.image_jobs
    }

    /// `morf.audio`.
    pub(crate) fn audio_mut(&mut self) -> &mut Option<crate::api::media::audio::AudioHost> {
        &mut self.audio
    }

    /// `morf.toplevels`.
    pub(crate) fn toplevels_mut(
        &mut self,
    ) -> &mut Option<crate::api::desktop::toplevels::ToplevelHost> {
        &mut self.toplevels
    }

    /// `morf.process.spawn` children and `morf.socket.connect` connections.
    pub(crate) fn io_mut(&mut self) -> &mut crate::api::io::io::IoHub {
        &mut self.io
    }

    /// The same, to count.
    pub(crate) fn io(&self) -> &crate::api::io::io::IoHub {
        &self.io
    }

    /// How much image work is on its way, for a test to watch drain.
    #[cfg(test)]
    pub(crate) fn image_jobs(&self) -> &crate::runtime::image_jobs::ImageJobs {
        &self.image_jobs
    }
}
