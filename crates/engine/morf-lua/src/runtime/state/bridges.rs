//! Two parts of the state lent at once, for work that reads one while it
//! changes the other.

use super::*;

use crate::api::desktop::toplevels::ToplevelHost;
use crate::api::input::overlay::OverlayState;
use crate::api::media::audio::AudioHost;
use crate::api::ui::channels::Channels;
use crate::runtime::terminals::TerminalHub;
use morf_runtime::Engine;
use morf_runtime::layout::TransformWatches;

impl ReactiveState {
    /// The overlays, beside the scene they sit in.
    pub(crate) fn overlays_and_scene(&mut self) -> (&mut OverlayState, &Scene) {
        (&mut self.overlays, self.engine.scene())
    }

    /// The channels, beside the engine their signals and paths live in.
    pub(crate) fn channels_and_engine(&mut self) -> (&mut Channels, &mut Engine) {
        (&mut self.channels, &mut self.engine)
    }

    /// The `ui.watch_transform` watches, beside the engine they observe.
    pub(crate) fn transform_watchers_and_engine(&mut self) -> (&mut TransformWatches, &Engine) {
        (&mut self.transform_watchers, &self.engine)
    }

    /// The terminals, beside the scene they draw into.
    pub(crate) fn terminals_and_scene(&mut self) -> (&mut TerminalHub, &mut Scene) {
        (&mut self.terminals, self.engine.scene_mut())
    }

    /// `morf.audio`, beside the engine its rows and signals go to.
    pub(crate) fn audio_and_engine(&mut self) -> (Option<&mut AudioHost>, &mut Engine) {
        (self.audio.as_mut(), &mut self.engine)
    }

    /// `morf.toplevels`, beside the engine its rows and signals go to.
    pub(crate) fn toplevels_and_engine(&mut self) -> (Option<&mut ToplevelHost>, &mut Engine) {
        (self.toplevels.as_mut(), &mut self.engine)
    }
}
