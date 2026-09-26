use morf_lua::Runtime;
use morf_wayland::{LayerClient, PRIMARY_LAYER};
use std::time::Duration;

use crate::{surface_layers::*, surfaces::*};

/// How a surface decides which frame callbacks it can afford to paint on.
///
/// A worker paints when the compositor says it may, and on a machine that can
/// keep up that is exactly right. When it cannot — three fullscreen overlays on
/// one GPU, say — every worker still tries for every callback, they contend,
/// and the loser does not degrade to a slower steady rate: it misses deadlines
/// irregularly. A steady thirty reads as smooth; thirty that arrives in bursts
/// of sixty and gaps reads as stutter, which is worse than either.
///
/// So a surface that cannot paint inside one refresh deliberately paints on
/// every second callback, or every third, and keeps that cadence. It gives up
/// frames it was going to lose anyway, and gets an even rhythm in exchange.
#[derive(Debug)]
pub(crate) struct FramePacer {
    /// Typical cost of producing one frame: the median of `recent`.
    pub(crate) cost: Option<Duration>,
    /// The last few paints' costs, oldest first.
    recent: std::collections::VecDeque<Duration>,
    /// Callbacks seen since the last paint, or `None` when the surface is at
    /// rest and the next callback should paint whatever the cadence was.
    pub(crate) waited: Option<u32>,
}

/// How many recent paints the cost is the median of.
///
/// A median, not an average: one slow frame -- a layout of the whole tree, a
/// shader compiled on demand, a present that waited on a busy GPU -- moved an
/// average far enough to drop a desk of 60 Hz screens to fifteen frames a
/// second for the rest of the motion. Half of these must be slow before the
/// cadence changes, which a real change reaches within a few frames.
pub(crate) const COST_WINDOW: usize = 7;

impl FramePacer {
    pub(crate) fn new() -> Self {
        Self {
            cost: None,
            recent: std::collections::VecDeque::with_capacity(COST_WINDOW),
            waited: None,
        }
    }

    /// Records what the last paint cost.
    pub(crate) fn observed(&mut self, cost: Duration) {
        if self.recent.len() == COST_WINDOW {
            self.recent.pop_front();
        }
        self.recent.push_back(cost);
        let mut sorted: Vec<Duration> = self.recent.iter().copied().collect();
        sorted.sort_unstable();
        self.cost = Some(sorted[sorted.len() / 2]);
    }

    /// Callbacks this surface lets pass between paints.
    ///
    /// One while it fits inside a refresh, two when it needs up to two, and so
    /// on. Capped, because a surface that has become very slow should keep
    /// painting occasionally rather than stop.
    pub(crate) fn interval(&self, refresh: Duration) -> u32 {
        const SLOWEST: u32 = 4;
        let Some(cost) = self.cost else {
            return 1;
        };
        if refresh.is_zero() {
            return 1;
        }
        let needed = cost.as_secs_f64() / refresh.as_secs_f64();
        // A frame that only just fits is not worth halving the rate for.
        (needed * 0.9).ceil().clamp(1.0, f64::from(SLOWEST)) as u32
    }

    /// Whether this callback is one the surface paints on.
    pub(crate) fn due(&mut self, refresh: Duration) -> bool {
        // A surface with no cadence yet — new, or just woken — paints at once.
        // Making the first frame of motion wait is the one delay nobody can
        // afford, because it is the one the eye is waiting for.
        let Some(waited) = self.waited else {
            self.waited = Some(0);
            return true;
        };
        if waited + 1 >= self.interval(refresh) {
            self.waited = Some(0);
            return true;
        }
        self.waited = Some(waited + 1);
        false
    }

    /// Forgets where in the cadence the surface was, for when it stops moving.
    ///
    /// Returns whether a paint is still owed: the callback the motion ended on
    /// was one the cadence gave up, so the frame it landed on was advanced but
    /// never drawn. Without that paint the surface stops on the last frame it
    /// did draw — a tile left between its old colour and its new one, and
    /// nothing moving to ever correct it.
    pub(crate) fn rest(&mut self) -> bool {
        self.waited.take().is_some_and(|waited| waited > 0)
    }
}

/// One frame callback for the shell's own surface: motion advances, and the
/// pacer decides whether this callback is painted on.
pub(crate) fn primary_frame(
    runtime: &mut Runtime,
    client: &mut LayerClient,
    state: &mut SurfaceEventState,
    time_ms: u32,
) -> Result<bool, String> {
    let mut repaint = false;
    let delta = animation_delta(state.last_frame, time_ms);
    let frame = runtime
        .tick_frame_animations(delta)
        .map_err(|error| error.to_string())?;
    // Carried forward only while motion continues, so the next run of
    // animation starts from a clean timebase rather than inheriting
    // however long the shell was idle.
    state.last_frame = frame.active.then_some(time_ms);
    // The callbacks themselves are the clock: whatever rate the
    // compositor offers this output is the rate to pace against.
    if !delta.is_zero() {
        state.refresh = delta;
    }
    // A shader reading the clock is motion like any other: it makes
    // the frame *advance*, and the pacer still decides which callbacks
    // are painted on. Forcing a repaint outside this path would spin as
    // fast as the event loop turns rather than at the output's rate.
    let advanced = frame.active || frame.changed > 0 || state.animating_shaders;
    if advanced {
        // A surface that cannot paint inside one refresh paints on
        // every second callback instead, and keeps that cadence rather
        // than missing deadlines at random.
        if state.pacer.due(state.refresh) {
            repaint = true;
        } else {
            // The next callback is asked for by painting, so a skipped
            // frame has to ask for itself — otherwise the compositor
            // has nothing outstanding, never calls back, and the
            // surface stops dead on the first frame it gives up.
            client.request_layer_frame(PRIMARY_LAYER);
            client.commit_layer(PRIMARY_LAYER);
        }
    } else if state.pacer.rest() {
        // The motion landed on a callback the cadence skipped; this is the
        // paint that shows where it landed.
        repaint = true;
    }
    // `MORF_FRAME_LOG=2`: every callback, and what the pacer made of it --
    // whether a slow cadence is the compositor's or this surface's choice.
    if advanced && crate::paint::frame_split_wanted() {
        eprintln!(
            "{} {}: callback after {:.1} ms: {} (cost {:.1} ms, every {})",
            morf_lua::profile::stamp(),
            std::thread::current().name().unwrap_or("?"),
            delta.as_secs_f64() * 1000.0,
            if repaint { "paint" } else { "skip" },
            state.pacer.cost.map_or(0.0, |cost| cost.as_secs_f64() * 1000.0),
            state.pacer.interval(state.refresh),
        );
    }
    // Configured layer surfaces have no clock of their own; the shell's
    // tick is what tells them a repaint is due, and a surface that is
    // already idle needs a frame callback to come back on.
    if advanced {
        for surface in state.layer_surfaces.values_mut() {
            if surface.updates_enabled && !surface.needs_paint {
                surface.needs_paint = true;
                // Committed, or the request waits for a commit that an idle
                // surface never makes and the callback never comes.
                client.request_layer_frame(window_layer_id(surface.id));
                client.commit_layer(window_layer_id(surface.id));
            }
        }
    }

    Ok(repaint)
}
