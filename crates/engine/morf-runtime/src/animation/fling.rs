//! A fling: a property thrown at a speed, coasting where friction, gravity
//! and its bounds take it.

use morf_scene::Physics;

/// Presets from Animato, named so a configuration need not tune friction by
/// hand for the ordinary cases: `(friction, min_velocity)`.
pub fn preset(name: &str) -> Option<(f64, f64)> {
    Some(match name {
        // Long-running, for scrolling a list or a canvas.
        "smooth" => (1400.0, 2.0),
        // Short and responsive, for something being directly manipulated.
        "snappy" => (3600.0, 4.0),
        // Slow to give up, for large panels.
        "heavy" => (800.0, 1.0),
        _ => return None,
    })
}

/// The friction and resting speed a fling starts from: its preset's, or the
/// smooth one's when it names none.
pub fn preset_or_default(name: Option<&str>) -> Result<(f64, f64), String> {
    match name {
        None => Ok((1400.0, 2.0)),
        Some(name) => preset(name).ok_or_else(|| {
            format!("unknown fling preset `{name}`; expected smooth, snappy, or heavy")
        }),
    }
}

/// What a fling's settings make of its motion. Both bounds or neither.
pub fn decay(
    friction: f64,
    min_velocity: f64,
    gravity: f64,
    bounce: f64,
    min: Option<f64>,
    max: Option<f64>,
) -> Result<Physics, String> {
    let bounds = match (min, max) {
        (Some(low), Some(high)) => Some((low, high)),
        (None, None) => None,
        _ => return Err("a fling bound needs both `min` and `max`".to_owned()),
    };
    Ok(Physics::Decay {
        friction,
        min_velocity,
        bounds,
        gravity,
        restitution: bounce,
    })
}
