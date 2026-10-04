//! Motion while the compositor sends the shell's own surface no frame
//! callbacks: when the wall clock has to tick it, and the tick itself.

use morf_app::Backend;
use morf_app::PRIMARY_LAYER;
use morf_lua::Runtime;
use std::time::{Duration, Instant};

use crate::host::windows::Kind;
use crate::{paint::*, surfaces::*};

/// Advances motion from the wall clock while the shell's own surface gets no
/// frame callbacks.
///
/// Motion is ticked by that surface's callbacks, so when a nested compositor
/// hides it -- cage shows one toplevel, and without layer-shell every surface
/// is one -- every animation on every surface stopped. Past a few refreshes
/// with its callback outstanding, the clock ticks at the measured refresh and
/// the other surfaces paint on their own callbacks. The next real callback
/// takes over again from a clean timebase.
pub(super) fn advance_without_callbacks(
    runtime: &mut Runtime,
    client: &dyn Backend,
    state: &mut SurfaceEventState,
) -> Result<(), String> {
    let stalled = client
        .layer_frame_wait(PRIMARY_LAYER)
        .is_some_and(|wait| wait > frame_stall(state.refresh));
    if !stalled {
        state.fallback_tick = None;
        return Ok(());
    }
    let now = Instant::now();
    let delta = match state.fallback_tick {
        Some(last) if now - last < state.refresh => return Ok(()),
        Some(last) => (now - last).min(Duration::from_millis(MAX_FRAME_DELTA_MS.into())),
        None => Duration::ZERO,
    };
    state.fallback_tick = Some(now);
    state.last_frame = None;
    let frame = runtime
        .tick_frame_animations(delta)
        .map_err(|error| error.to_string())?;
    if frame.active || frame.changed > 0 || state.animating_shaders {
        state.primary_deferred = true;
        for surface in state
            .windows
            .of_kind_mut(Kind::Layer)
            .map(|(_, surface)| surface)
        {
            if surface.updates_enabled {
                surface.needs_paint = true;
                paint_layer_surface(runtime, client, surface)?;
            }
        }
    }
    Ok(())
}

/// When the wall clock next has to tick motion, while the compositor sends
/// this output's surface no frame callbacks: the stall check a few refreshes
/// after the callback was asked for, then every refresh. Nothing while no
/// callback is outstanding or nothing moves -- the callbacks themselves, or
/// the event that starts the motion, wake the loop then.
pub(super) fn motion_deadline(
    runtime: &Runtime,
    client: &dyn Backend,
    state: &SurfaceEventState,
) -> Option<Instant> {
    let waiting = client.layer_frame_wait(PRIMARY_LAYER)?;
    let now = Instant::now();
    if !(state.animating_shaders || runtime.has_motion()) {
        // Still, a paint owed and waiting on the callback comes due once
        // the callback is overdue (`owed_paint_due`).
        if !state.primary_deferred {
            return None;
        }
        let stall = frame_stall(state.refresh);
        let after_wait = now + stall.saturating_sub(waiting);
        let after_forced = state.forced_paint.map_or(now, |at| at + stall);
        return Some(after_wait.max(after_forced) + Duration::from_millis(1));
    }
    Some(match state.fallback_tick {
        Some(last) => last + state.refresh,
        // A hair past the threshold, so the check finds it crossed.
        None => now + frame_stall(state.refresh).saturating_sub(waiting) + Duration::from_millis(1),
    })
}
