//! `Runtime::poll_services`, one turn of the services: collect what is due
//! with the state borrowed, keep Timers and Loaders in step, then run the
//! callbacks.

use morf_system::{GreetdEvent, PamEvent};
use std::time::Duration;

use morf_scene::{NodeHandle, Value as SceneValue};

use crate::{
    reactive_execute::*, runtime_helpers::*, scene_bindings::*, state::*, surface_types::*,
    types::*, views::*,
};
use morf_runtime::Handler;
use morf_runtime::timers::{DueTimer, Timer};

mod collect;
mod definitions;
mod deliver;

use collect::{collect_pam_results, drain};
use definitions::{reconcile_loaders, reconcile_timers};

/// How long a preloading Loader waits for the scene to be still before it
/// builds anyway: an animation that never ends (a spinner, a visualiser)
/// must not hold a preload back for ever.
pub(crate) const PRELOAD_PATIENCE: Duration = Duration::from_millis(1500);

/// What one turn collects while the state is borrowed, to be run once it is
/// let go.
#[derive(Default)]
struct Collected {
    ready: Vec<(Handler, bool, Result<(), morf_system::PamError>)>,
    timers: Vec<DueTimer>,
    dbus_signals: Vec<(
        u64,
        Handler,
        crate::state_pending::DbusSignalKind,
        morf_io::DbusSignalEvent,
    )>,
    dbus_replies: Vec<(Handler, Result<morf_io::DbusValue, String>)>,
    dbus_calls: Vec<(Handler, morf_io::DbusCall)>,
    pam_messages: Vec<(Handler, PamEvent)>,
    greetd_messages: Vec<(Handler, GreetdEvent)>,
    udev_events: Vec<(Handler, morf_system::UdevEvent)>,
    status_updates: Vec<(Handler, Vec<morf_system::StatusNotifierAddress>)>,
    http_answers: Vec<(
        Handler,
        Result<morf_io::HttpResponse, String>,
        String,
        crate::api_http::JsonKinds,
        std::rc::Rc<crate::api_http::HttpHandleState>,
    )>,
    io_calls: Vec<crate::api_io::IoCall>,
    io_more: bool,
    watch_calls: Vec<crate::api_watch::WatchCall>,
    watch_more: bool,
    retained_destroys: Vec<NodeHandle>,
    transform_callbacks: Vec<(Handler, u64)>,
}

impl Runtime {
    /// Polls native service jobs and runs completed callbacks with bounded fuel.
    pub fn poll_services(&mut self) -> bool {
        // Outside the arena, before this turn's Lua: compile what turned hot.
        self.service_jit();
        let _bookkeeping = crate::profile::span(|| "engine: services bookkeeping".to_owned());
        self.flush_lint();
        // The loop wakes when the caret is due to turn over
        // (`Runtime::next_deadline`), so it blinks without a timer of its own.
        let devices =
            crate::profile::span(|| "engine: appearance, audio, terminals, images".to_owned());
        let blinked = self.blink_text_inputs();
        let appearance_changed = self.poll_appearance();
        let audio_changed = self.poll_audio();
        let terminals_changed = self.poll_terminals();
        let images_changed = self.poll_images();
        let palettes_changed = self.poll_palette_listeners();
        let shared_changed = self.poll_shared();
        let channels_changed = self.poll_channels();
        let focus_changed = self.poll_overlays() | self.check_focus() | self.poll_gestures();
        drop(devices);
        let mut collected = Collected::default();
        let mut loaders = Vec::new();
        let mut preloads = Vec::new();
        let mut loader_drops = Vec::new();
        let mut service_changed = false;
        {
            let _collect = crate::profile::span(|| "engine: timers, loaders and buses".to_owned());
            let mut state = self.reactive.borrow_mut();
            collect_pam_results(&mut state, &mut collected);
            // Scene-backed service definitions only need reconciling after
            // a scene write. Native timers still fire and buses still drain
            // below on every poll. Animated Timer intervals invalidate this
            // checkpoint in tick_animations, including their final tick.
            let definitions_changed =
                state.revisions.scene_revision != state.revisions.service_definitions_revision;
            state.revisions.service_definitions_revision = state.revisions.scene_revision;
            service_changed |= reconcile_timers(&mut state, definitions_changed);
            service_changed |= reconcile_loaders(
                &mut state,
                definitions_changed,
                &mut loaders,
                &mut preloads,
                &mut loader_drops,
            );
            drain(&mut state, &mut collected);
        }
        let letting_go = crate::profile::span(|| "engine: letting go of nodes".to_owned());
        for node in std::mem::take(&mut collected.retained_destroys) {
            self.lua
                .enter(|ctx| finish_retained_destroy(&self.reactive, ctx, self.limits, node));
            service_changed = true;
        }
        for &node in &loader_drops {
            self.lua
                .enter(|ctx| drop_retainable(&self.reactive, ctx, self.limits, node));
        }
        drop(letting_go);
        service_changed |= self.build_loaders(loaders, preloads);
        service_changed |= {
            let _span = crate::profile::span(|| "engine: views and repeaters".to_owned());
            self.sync_pending_views()
        };
        // Whether a repaint is owed is decided after the callbacks below have
        // run, by asking whether the scene actually changed. A callback merely
        // firing is not a reason to render: a 16ms timer that polls a file and
        // finds it unchanged would otherwise force a full render of every
        // output sixty times a second, forever.
        let (revision_before, hidden_before) = {
            let state = self.reactive.borrow();
            (
                state.revisions.scene_revision,
                state.revisions.hidden_revisions,
            )
        };
        // Timers, loaders and views are in step with the scene as it is now;
        // what the callbacks below change is for the next turn to pick up.
        self.reactive.borrow_mut().revisions.polled_revision = revision_before;
        let service_changed = service_changed
            || appearance_changed
            || audio_changed
            || terminals_changed
            || images_changed
            || palettes_changed
            || shared_changed
            || channels_changed
            || focus_changed
            || blinked
            || !collected.transform_callbacks.is_empty();
        self.deliver(collected);
        {
            let _span = crate::profile::span(|| "engine: image jobs".to_owned());
            self.poll_image_jobs();
        }
        // A source set outright, not animated, is followed at once too.
        crate::state::apply_follows(&mut self.reactive.borrow_mut());
        // Changes to nodes nothing shows are not painted: a hidden panel's
        // chart that follows a counter every second would otherwise draw
        // every output every second for a picture nobody sees.
        let state = self.reactive.borrow();
        let bumps = state.revisions.scene_revision.wrapping_sub(revision_before);
        let hidden = state.revisions.hidden_revisions.wrapping_sub(hidden_before);
        service_changed || bumps > hidden
    }
}

impl Runtime {
    /// Takes the nodes destroyed since the last frame, and drops what this
    /// crate holds for them on the way past.
    ///
    /// The transform tracker is reachable from here; the caches in the render
    /// backend are not, so the list is handed back for the caller to finish
    /// the job. Nobody else has both the scene and those caches in scope.
    pub fn take_removed_nodes(&self) -> Vec<NodeHandle> {
        let mut state = self.reactive.borrow_mut();
        let removed = state.scene.take_removed_nodes();
        if !removed.is_empty() {
            let morf_runtime::Engine {
                scene,
                transform_tracker,
                ..
            } = &mut state.engine;
            transform_tracker.retain_scene(&*scene);
        }
        removed
    }
}

/// Whether a timer collected as due should still fire, now that everything
/// before it in this turn has run.
///
/// A `Timer` node must still exist and still be a timer — not removed,
/// alone or with an ancestor such as a Loader letting its item go — and a
/// repeating one must still be running. A timer with no node must not have
/// been cancelled. A one-shot fires at most once either way.
fn timer_still_due(
    state: &mut ReactiveState,
    id: u64,
    node: Option<NodeHandle>,
    repeat: bool,
) -> bool {
    let settled = state.timers.settle(id, repeat);
    if let Some(node) = node {
        if !state.timer_callbacks.contains_key(&node) {
            return false;
        }
        let Ok(running) = state.scene.bool_value(node, "running") else {
            return false;
        };
        if repeat && !running {
            return false;
        }
    }
    settled
}

/// Whether `MORF_WAKE_LOG` asks for wakes -- and so the timers behind them --
/// to be named.
fn wake_log_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| {
        std::env::var_os("MORF_WAKE_LOG").is_some_and(|value| !value.is_empty() && value != "0")
    })
}
