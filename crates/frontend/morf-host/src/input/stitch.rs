//! One finger out of two contacts: a short contact, lifted, and a new one
//! landing near it a few milliseconds later. A tap that lifts where a pan
//! could still take its finger waits [`GAP`] before its release, its click
//! and its tap outside an overlay are delivered. A contact that lands within
//! [`REACH`] of it meanwhile is held until it shows what it is:
//!
//! - moving [`SLOP`] from where it landed, it is the first finger going on:
//!   the tap's click is dropped, and the first press and the pan's origin are
//!   kept, so the pan sees one contact. No new node is pressed.
//! - lifting where it landed, or resting there past `GAP`, it was a finger of
//!   its own: the tap is delivered, and then this contact from its landing.
//!
//! Raw touch events keep the contacts the touchscreen reported: a finger
//! carried on is told as the first contact's release and the new one's press,
//! both on the node the first was pressed on.

use std::time::Duration;

use morf_app::{Event, WindowId};
use morf_layout::{Hit, Layout};
use morf_lua::{EventPoint, Runtime, UiEvent};
use morf_scene::{Element, NodeHandle};

use crate::host::surfaces::{PointerInput, SurfaceLayouts};
use crate::input::pan::EDGE;
use crate::input::touch::{TAP_TRAVEL, TOUCH_BUTTON, handle_touch_event, outside_tap};

/// How long a lifted tap waits for its finger to land again.
pub const GAP: Duration = Duration::from_millis(60);
/// How far from where a tap lifted a contact may land and be its finger.
pub const REACH: f64 = 320.0;
/// How far a held landing moves before it is the first finger going on.
const SLOP: f64 = 8.0;

/// A tap lifted and not yet delivered.
pub struct Lifted {
    id: i32,
    surface: WindowId,
    hit: Hit,
    origin: (f64, f64),
    at: (f64, f64),
    travel: f64,
    click: bool,
    overlay: bool,
    until: Duration,
    landing: Option<Landing>,
}

/// A contact that landed near a held tap, not yet told apart from it.
struct Landing {
    id: i32,
    from: (f64, f64),
    last: (f64, f64),
    time_ms: Option<u32>,
    until: Duration,
}

/// Whether a pan could take a finger that landed at `(x, y)` on `node`:
/// something on its way up pans or scrolls, and nothing before that keeps
/// its every drag; or it landed by an edge its surface's root pulls from.
fn pannable(runtime: &Runtime, layout: &Layout, node: NodeHandle, (x, y): (f64, f64)) -> bool {
    let scene = runtime.scene();
    let mut at = Some(node);
    while let Some(current) = at {
        if runtime.handles(current, UiEvent::Panned)
            || scene.element(current).ok() == Some(Element::Flickable)
        {
            return true;
        }
        if (runtime.drags_along(current, true) && runtime.drags_along(current, false))
            || runtime.is_text_input(current)
            || runtime.is_terminal(current)
        {
            return false;
        }
        at = scene.parent(current).ok().flatten();
    }
    let Some(root) = scene.root_of(node) else {
        return false;
    };
    runtime.handles(root, UiEvent::EdgePanned)
        && layout.surface_rect(&scene, root).is_some_and(|r| {
            x - r.x <= EDGE
                || r.x + r.width - x <= EDGE
                || y - r.y <= EDGE
                || r.y + r.height - y <= EDGE
        })
}

fn point_on(
    runtime: &Runtime,
    layouts: &dyn SurfaceLayouts,
    surface: WindowId,
    node: NodeHandle,
    (x, y): (f64, f64),
) -> EventPoint {
    let local = layouts
        .layout_of(surface)
        .map(|layout| layout.local_point(&runtime.scene(), node, x, y))
        .unwrap_or((x, y));
    EventPoint::new((x, y), local).with_button(TOUCH_BUTTON)
}

/// Holds back the lift of `id` at `at` when it is a tap that a pan could
/// still take its finger from. Called with the contact still among the
/// touches; `origin` is where it landed. Returns whether it is held.
#[allow(clippy::too_many_arguments)]
pub fn hold(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    layouts: &dyn SurfaceLayouts,
    surface: WindowId,
    id: i32,
    (x, y): (f64, f64),
    origin: Option<(f64, f64)>,
    suppress_tap: bool,
) -> Result<bool, String> {
    let Some(&(touch_surface, hit, _, _, travel)) = input.touches.get(&id) else {
        return Ok(false);
    };
    let Some(layout) = layouts.layout_of(surface) else {
        return Ok(false);
    };
    let origin = origin.unwrap_or((x, y));
    if input.touches.len() != 1
        || suppress_tap
        || travel > TAP_TRAVEL
        || touch_surface != surface
        || !super::pan::undecided(input, id)
        || !pannable(runtime, layout, hit.node, origin)
    {
        return Ok(false);
    }
    let under = layout
        .hit_test_accepting(&runtime.scene(), x, y, &|node| {
            runtime.accepts_pointer_button(node, TOUCH_BUTTON)
        })
        .map_err(|error| error.to_string())?;
    let overlay = input
        .touch_overlay_origins
        .remove(&id)
        .is_some_and(|(at, sx, sy)| at == surface && (x - sx).hypot(y - sy) <= TAP_TRAVEL);
    input.touches.remove(&id);
    let until = runtime.gesture_clock() + GAP;
    runtime.hold_touch(Some(until));
    input.lifted = Some(Lifted {
        id,
        surface,
        hit,
        origin,
        at: (x, y),
        travel,
        click: under.map(|under| under.node) == Some(hit.node),
        overlay,
        until,
        landing: None,
    });
    Ok(true)
}

/// Delivers a held tap as its lift would have been: the overlays' outside
/// tap, the release and, when `click`, the click.
fn release(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    layouts: &dyn SurfaceLayouts,
    lifted: Lifted,
    click: bool,
) -> Result<bool, String> {
    let mut repaint = super::pan::up(runtime, input, Some(lifted.id), None).1;
    let (node, (x, y)) = (lifted.hit.node, lifted.at);
    if click
        && lifted.overlay
        && let Some(layout) = layouts.layout_of(lifted.surface)
    {
        repaint |= outside_tap(runtime, layout, x, y)?;
    }
    let point = point_on(runtime, layouts, lifted.surface, node, lifted.at);
    repaint |= runtime.dispatch_touch_event(node, UiEvent::TouchReleased, lifted.id, point);
    repaint |= runtime.dispatch_pointer(node, UiEvent::Released, point, (0.0, 0.0));
    if click && lifted.click {
        repaint |= runtime.dispatch_pointer(node, UiEvent::Clicked, point, (0.0, 0.0));
    }
    Ok(repaint)
}

/// Delivers the held tap now, and then the contact that landed beside it,
/// if one did, as a finger of its own from where it landed.
pub fn deliver(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    layouts: &dyn SurfaceLayouts,
) -> Result<bool, String> {
    let Some(mut lifted) = input.lifted.take() else {
        return Ok(false);
    };
    runtime.hold_touch(None);
    let (surface, landing) = (lifted.surface, lifted.landing.take());
    let mut repaint = release(runtime, input, layouts, lifted, true)?;
    if let Some(landing) = landing {
        let (x, y) = landing.from;
        let (id, time_ms) = (landing.id, landing.time_ms);
        let down = Event::TouchDown {
            surface,
            id,
            x,
            y,
            time_ms,
        };
        repaint |= handle_touch_event(runtime, input, layouts, down)?;
        if landing.last != landing.from {
            let (x, y) = landing.last;
            let motion = Event::TouchMotion {
                surface,
                id,
                x,
                y,
                time_ms,
            };
            repaint |= handle_touch_event(runtime, input, layouts, motion)?;
        }
    }
    Ok(repaint)
}

/// Delivers a held tap whose time has passed; with nothing held, makes sure
/// the loop no longer wakes for one.
pub fn flush(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    layouts: &dyn SurfaceLayouts,
) -> Result<bool, String> {
    let Some(lifted) = input.lifted.as_ref() else {
        runtime.hold_touch(None);
        return Ok(false);
    };
    let due = lifted
        .landing
        .as_ref()
        .map_or(lifted.until, |landing| landing.until);
    if runtime.gesture_clock() < due {
        return Ok(false);
    }
    deliver(runtime, input, layouts)
}

/// A contact landing while a tap is held. Kept back when it may be the tap's
/// finger landing again; otherwise the tap, and a landing held before it, are
/// delivered first. Returns (kept back, repaint).
pub fn landed(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    layouts: &dyn SurfaceLayouts,
    surface: WindowId,
    id: i32,
    (x, y): (f64, f64),
    time_ms: Option<u32>,
) -> Result<(bool, bool), String> {
    let alone = input.touches.is_empty();
    let Some(lifted) = input.lifted.as_mut() else {
        return Ok((false, false));
    };
    if alone
        && lifted.landing.is_none()
        && lifted.surface == surface
        && (x - lifted.at.0).hypot(y - lifted.at.1) <= REACH
    {
        let until = runtime.gesture_clock() + GAP;
        lifted.landing = Some(Landing {
            id,
            from: (x, y),
            last: (x, y),
            time_ms,
            until,
        });
        runtime.hold_touch(Some(until));
        return Ok((true, false));
    }
    Ok((false, deliver(runtime, input, layouts)?))
}

/// A held landing's motion. Within `SLOP` of where it landed it stays held;
/// past it, the held tap's finger goes on as this contact, and the motion is
/// then the carried finger's. Returns (kept back, repaint).
pub fn moved(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    layouts: &dyn SurfaceLayouts,
    id: i32,
    (x, y): (f64, f64),
) -> (bool, bool) {
    let Some(landing) = input
        .lifted
        .as_mut()
        .and_then(|lifted| lifted.landing.as_mut())
        .filter(|landing| landing.id == id)
    else {
        return (false, false);
    };
    landing.last = (x, y);
    let from = landing.from;
    if (x - from.0).hypot(y - from.1) < SLOP {
        return (true, false);
    }
    let Some(lifted) = input.lifted.take() else {
        return (false, false);
    };
    runtime.hold_touch(None);
    let (surface, node) = (lifted.surface, lifted.hit.node);
    input.touches.insert(
        id,
        (surface, lifted.hit, lifted.at.0, lifted.at.1, lifted.travel),
    );
    input.touch_origins.insert(id, lifted.origin);
    super::pan::carry_on(input, lifted.id, id);
    let lift = point_on(runtime, layouts, surface, node, lifted.at);
    let land = point_on(runtime, layouts, surface, node, from);
    let mut repaint = runtime.dispatch_touch_event(node, UiEvent::TouchReleased, lifted.id, lift);
    repaint |= runtime.dispatch_touch_event(node, UiEvent::TouchPressed, id, land);
    (false, repaint)
}

/// A held landing lifting where it landed: it was a finger of its own. The
/// held tap is delivered, then this contact from its landing, whose lift
/// then goes on as any other.
pub fn parted(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    layouts: &dyn SurfaceLayouts,
    id: i32,
) -> Result<bool, String> {
    let held = input
        .lifted
        .as_ref()
        .and_then(|lifted| lifted.landing.as_ref())
        .is_some_and(|landing| landing.id == id);
    if !held {
        return Ok(false);
    }
    deliver(runtime, input, layouts)
}

/// Every contact cancelled: a held tap is released without its click or its
/// outside tap, and a held landing is forgotten.
pub fn cancel(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    layouts: &dyn SurfaceLayouts,
) -> Result<bool, String> {
    let Some(mut lifted) = input.lifted.take() else {
        return Ok(false);
    };
    runtime.hold_touch(None);
    lifted.landing = None;
    release(runtime, input, layouts, lifted, false)
}
