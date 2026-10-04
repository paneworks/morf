//! Theme colours easing from the value on show to the one last written.

use std::time::Duration;

use morf_scene::reactive::SignalId;
use morf_scene::{Color, ColorSpace, Easing, HueDirection};

/// A theme token's colour on its way to the one last written to it.
pub struct ThemeFade {
    pub signal: SignalId,
    pub from: Color,
    pub to: Color,
    pub elapsed: Duration,
    pub duration: Duration,
    pub easing: Easing,
}

impl ThemeFade {
    /// The colour on show now, and whether the fade is over.
    pub fn colour(&self) -> (Color, bool) {
        let progress = if self.duration.is_zero() {
            1.0
        } else {
            (self.elapsed.as_secs_f64() / self.duration.as_secs_f64()).min(1.0)
        };
        let colour = self.easing.interpolate_color(
            progress,
            self.from,
            self.to,
            ColorSpace::Oklab,
            HueDirection::Shorter,
        );
        (colour, progress >= 1.0)
    }
}

/// Moves every fade on by `delta`: the colour each token shows now, in
/// order. A fade that has arrived is dropped after its last colour.
pub fn advance(fades: &mut Vec<ThemeFade>, delta: Duration) -> Vec<(SignalId, Color)> {
    let mut writes = Vec::new();
    fades.retain_mut(|fade| {
        fade.elapsed += delta;
        let (colour, done) = fade.colour();
        writes.push((fade.signal, colour));
        !done
    });
    writes
}
