//! A surface turning with its output: the frame it showed last, held on it
//! where it stood while the configuration lays itself out the other way up,
//! then turned upright and faded into what it became.

use std::time::{Duration, Instant};

use morf_app::Output;
use morf_lua::CoverPose;
use morf_render::WgpuBackend;

/// The name the held frame is published under, for the cover's picture.
pub const COVER: &str = "morf.cover";
/// The `ui.Image` source that draws it.
pub const COVER_SOURCE: &str = "gpu:morf.cover";
/// How long the picture takes to turn upright and fade.
const TURN: Duration = Duration::from_millis(450);
/// How long it waits for a rebuild the configuration asked for.
const HOLD: Duration = Duration::from_secs(3);
/// How long it waits for one when nothing was asked for.
const GRACE: Duration = Duration::from_millis(120);

/// A surface's turn, from the frame before it to the one after.
pub struct Turning {
    /// The held frame's size, in the logical pixels it was drawn at.
    size: (f64, f64),
    /// Clockwise degrees that keep it where it stood on the panel.
    angle: f64,
    phase: Phase,
}

enum Phase {
    /// Shown alone and still, while the configuration rebuilds.
    Held { since: Instant, rebuilding: bool },
    /// Turning upright and fading over the scene, from its first frame.
    Turning { started: Option<Instant> },
}

/// How the cover stands this frame, and whether it hides the scene.
pub struct Shown {
    pub pose: CoverPose,
    pub conceals: bool,
}

impl Turning {
    /// Holds the frame on `renderer`, drawn at `scale_120`, turned by
    /// `angle`; nothing when nothing was drawn yet.
    pub fn begin(renderer: &mut WgpuBackend, scale_120: u32, angle: f64) -> Option<Self> {
        let (width, height) = renderer.snapshot(COVER)?;
        let scale = f64::from(scale_120.max(1)) / 120.0;
        let size = (f64::from(width) / scale, f64::from(height) / scale);
        Some(Self::holding(size, angle, Instant::now()))
    }

    /// A frame `size` logical pixels big, held from `since`.
    fn holding(size: (f64, f64), angle: f64, since: Instant) -> Self {
        Self {
            size,
            angle,
            phase: Phase::Held {
                since,
                rebuilding: false,
            },
        }
    }

    /// The configuration asked to be rebuilt: the frame holds until it is.
    pub fn rebuilding(&mut self) {
        if let Phase::Held { rebuilding, .. } = &mut self.phase {
            *rebuilding = true;
        }
    }

    /// The configuration was rebuilt: the frame turns from its next paint.
    pub fn rebuilt(&mut self) {
        if matches!(self.phase, Phase::Held { .. }) {
            self.phase = Phase::Turning { started: None };
        }
    }

    /// Whether it still holds, so the scene under it is not drawn.
    pub fn held(&self) -> bool {
        matches!(self.phase, Phase::Held { .. })
    }

    /// How it stands at `now` on a surface `surface` logical pixels big;
    /// nothing once it has turned and faded.
    pub fn shown(&mut self, now: Instant, surface: (u32, u32)) -> Option<Shown> {
        if let Phase::Held { since, rebuilding } = self.phase {
            let waited = now.saturating_duration_since(since);
            if waited < if rebuilding { HOLD } else { GRACE } {
                return Some(Shown {
                    pose: self.pose(0.0, surface),
                    conceals: true,
                });
            }
            self.phase = Phase::Turning { started: None };
        }
        let Phase::Turning { started } = &mut self.phase else {
            return None;
        };
        let started = *started.get_or_insert(now);
        let progress = now.saturating_duration_since(started).as_secs_f64() / TURN.as_secs_f64();
        (progress < 1.0).then(|| Shown {
            pose: self.pose(progress, surface),
            conceals: false,
        })
    }

    /// The picture `progress` of the way from where it stood to upright,
    /// fitted to the surface at each end and gone at the last.
    fn pose(&self, progress: f64, surface: (u32, u32)) -> CoverPose {
        let (width, height) = self.size;
        let quarter = (self.angle / 90.0).round().rem_euclid(2.0) == 1.0;
        let held = if quarter {
            (height, width)
        } else {
            (width, height)
        };
        let fit = |(w, h): (f64, f64)| {
            (f64::from(surface.0) / w.max(1.0)).min(f64::from(surface.1) / h.max(1.0))
        };
        let (from, to) = (fit(held), fit(self.size));
        let turned = ease(progress);
        CoverPose {
            size: self.size,
            rotation: self.angle * (1.0 - turned),
            scale: from + (to - from) * turned,
            // Seen turning first, then gone over the second half.
            opacity: 1.0 - ease(((progress - FADE_FROM) / (1.0 - FADE_FROM)).max(0.0)),
        }
    }
}

/// How far into the turn the picture starts to fade.
const FADE_FROM: f64 = 0.3;

/// Easing in and out: the turn gathers, then settles.
fn ease(progress: f64) -> f64 {
    let t = progress.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// The shell's own surface is about to take `size` (physical pixels):
/// standing the other way up from the frame on it, the surface turned, and
/// that frame is held before anything is drawn at the new size.
pub fn before_resize(
    state: &mut crate::host::surfaces::SurfaceEventState,
    client: &dyn morf_app::Backend,
    name: &str,
    size: (u32, u32),
) {
    let to = transform_of(client.outputs(), name).or(state.transform);
    let from = std::mem::replace(&mut state.transform, to);
    if state.turning.as_ref().is_some_and(Turning::held) {
        return;
    }
    let angle = match (from, to) {
        (Some(from), Some(to)) => hold_angle(from, to),
        _ => 0.0,
    };
    let scale_120 = drawn_at(state, client);
    let Some(renderer) = state.painter.gpu() else {
        return;
    };
    let was = renderer.backend_mut().target_size();
    let upright = |(width, height): (u32, u32)| height > width;
    if was.0 == 0 || was.1 == 0 || upright(was) == upright(size) {
        return;
    }
    state.turning = Turning::begin(renderer.backend_mut(), scale_120, angle);
}

/// The output list changed: this output's transform with it is a turn,
/// held from the frame still on the surface; one held already takes the
/// angle it learned only now. The first transform heard is only noted.
pub fn screens_changed(
    state: &mut crate::host::surfaces::SurfaceEventState,
    client: &dyn morf_app::Backend,
    screens: &[Output],
    name: &str,
) -> bool {
    let Some(to) = transform_of(screens, name).filter(|to| Some(*to) != state.transform) else {
        return false;
    };
    let Some(from) = state.transform.replace(to) else {
        return false;
    };
    let angle = hold_angle(from, to);
    if let Some(turning) = state.turning.as_mut().filter(|turning| turning.held()) {
        turning.angle = angle;
        return true;
    }
    let scale_120 = drawn_at(state, client);
    let Some(renderer) = state.painter.gpu() else {
        return false;
    };
    state.turning = Turning::begin(renderer.backend_mut(), scale_120, angle);
    state.turning.is_some()
}

/// How the cover stands for the frame about to be painted; once it has
/// turned and faded, it is taken down and its picture let go.
pub fn shown(
    state: &mut crate::host::surfaces::SurfaceEventState,
    runtime: &mut morf_lua::Runtime,
    client: &dyn morf_app::Backend,
) -> Option<Shown> {
    let turning = state.turning.as_mut()?;
    let surface = client.layer_logical_size(morf_app::PRIMARY_LAYER)?;
    let shown = turning.shown(Instant::now(), surface);
    if shown.is_none() {
        state.turning = None;
        runtime.clear_cover();
        if let Some(renderer) = state.painter.gpu() {
            renderer.backend_mut().forget_texture(COVER);
        }
    }
    shown
}

/// The scale the frame on the surface was drawn at: its layout's, or the
/// surface's own when a reload left that to be worked out again.
fn drawn_at(
    state: &crate::host::surfaces::SurfaceEventState,
    client: &dyn morf_app::Backend,
) -> u32 {
    match state.layout.scale_120 {
        0 => client.primary_scale_120(),
        scale => scale,
    }
}

/// `name`'s transform among `outputs`.
pub fn transform_of(outputs: &[Output], name: &str) -> Option<&'static str> {
    outputs
        .iter()
        .find(|output| output.name.as_deref() == Some(name))
        .map(|output| output.transform)
}

/// The clockwise degrees that keep a frame drawn under transform `from`
/// standing where it was on the panel under `to`: the opposite of the turn
/// between them. Mirrored on one side and not the other, only a fade.
pub fn hold_angle(from: &str, to: &str) -> f64 {
    let quarters = |name: &str| -> Option<(i32, bool)> {
        let flipped = name.starts_with("flipped");
        let turn = match name.trim_start_matches("flipped").trim_start_matches('_') {
            "" | "normal" => 0,
            "90" => 1,
            "180" => 2,
            "270" => 3,
            _ => return None,
        };
        Some((turn, flipped))
    };
    match (quarters(from), quarters(to)) {
        (Some((a, false)), Some((b, false))) => f64::from((b - a + 1).rem_euclid(4) - 1) * 90.0,
        (Some((a, true)), Some((b, true))) => f64::from(-((b - a + 1).rem_euclid(4) - 1)) * 90.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_is_held_against_the_turn() {
        assert_eq!(hold_angle("normal", "90"), 90.0);
        assert_eq!(hold_angle("90", "normal"), -90.0);
        assert_eq!(hold_angle("normal", "270"), -90.0);
        assert_eq!(hold_angle("270", "normal"), 90.0);
        assert_eq!(hold_angle("90", "270"), 180.0);
        assert_eq!(hold_angle("normal", "normal"), 0.0);
        assert_eq!(hold_angle("normal", "flipped"), 0.0);
        assert_eq!(hold_angle("flipped", "flipped_90"), -90.0);
    }

    #[test]
    fn a_held_frame_waits_for_its_rebuild_then_turns_upright_and_goes() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        // A phone's frame, held a quarter turn round on the landscape screen.
        let mut turning = Turning::holding((1080.0, 2400.0), -90.0, start);
        turning.rebuilding();
        let held = turning.shown(at(500), (2400, 1080)).expect("held");
        assert!(held.conceals);
        assert_eq!(held.pose.rotation, -90.0);
        assert_eq!(held.pose.scale, 1.0);
        assert_eq!(held.pose.opacity, 1.0);

        turning.rebuilt();
        let first = turning.shown(at(600), (2400, 1080)).expect("turning");
        assert!(!first.conceals);
        assert_eq!(first.pose.rotation, -90.0);
        let midway = turning
            .shown(at(600) + TURN / 2, (2400, 1080))
            .expect("turning");
        assert!(midway.pose.rotation > -90.0 && midway.pose.rotation < 0.0);
        assert!(midway.pose.scale < 1.0 && midway.pose.opacity < 1.0);
        assert!(turning.shown(at(600) + TURN, (2400, 1080)).is_none());
    }

    #[test]
    fn a_frame_nobody_rebuilds_for_turns_after_a_moment() {
        let start = Instant::now();
        let mut turning = Turning::holding((1080.0, 2400.0), 90.0, start);
        assert!(turning.shown(start, (2400, 1080)).expect("held").conceals);
        let later = turning.shown(start + GRACE, (2400, 1080)).expect("turning");
        assert!(!later.conceals);
        assert!(!turning.held());
    }
}
