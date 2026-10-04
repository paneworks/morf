//! What a `behavior` table's settings mean: the kinds of motion, their
//! limits and the names their fields take. The scripting layer reads the
//! fields; these turn them into the scene's `Behavior` and `Physics`.

use std::time::Duration;

use morf_scene::{ColorSpace, HueDirection, Physics, Repeat, RotationDirection};

/// `kind = "spring"`.
pub fn spring(mass: f64, damping: f64, stiffness: f64, epsilon: f64) -> Physics {
    Physics::Spring {
        mass,
        damping,
        stiffness,
        epsilon,
    }
}

/// `kind = "smoothed"`: chases its target at up to `velocity` per second.
pub fn smoothed(velocity: f64) -> Physics {
    Physics::Smoothed { velocity }
}

/// A tween's `duration`, in milliseconds.
pub fn duration(milliseconds: f64) -> Result<Duration, String> {
    if milliseconds < 0.0 {
        return Err("behavior duration cannot be negative".to_owned());
    }
    Ok(Duration::from_secs_f64(milliseconds / 1_000.0))
}

/// A tween's `delay`, in milliseconds.
pub fn delay(milliseconds: f64) -> Result<Duration, String> {
    if milliseconds < 0.0 {
        return Err("behavior delay cannot be negative".to_owned());
    }
    Ok(Duration::from_secs_f64(milliseconds / 1_000.0))
}

/// A tween's `time_scale`.
pub fn time_scale(scale: f64) -> Result<f64, String> {
    if scale <= 0.0 {
        return Err("behavior time_scale must be greater than zero".to_owned());
    }
    Ok(scale)
}

/// What a behaviour's `loops` field held.
pub enum Loops {
    Absent,
    Count(f64),
    Name(String),
}

/// The `loops` field and the `alternate` (or older `ping_pong`) switch as
/// one repetition mode. `loops` is a pass count or a mode name; alternating
/// makes a count or an absent field its back-and-forth variant.
pub fn repeat(loops: Loops, alternating: bool) -> Result<Repeat, String> {
    let count = |value: f64| -> Result<u32, String> {
        if !value.is_finite() || value < 1.0 {
            return Err("behavior loops must be at least one pass".to_owned());
        }
        Ok(value as u32)
    };
    match loops {
        Loops::Absent if alternating => Ok(Repeat::PingPong),
        Loops::Absent => Ok(Repeat::Once),
        Loops::Count(value) => Ok(match alternating {
            true => Repeat::PingPongTimes(count(value)?),
            false => Repeat::Times(count(value)?),
        }),
        Loops::Name(name) => match name.as_str() {
            "once" => Ok(Repeat::Once),
            "forever" => Ok(Repeat::Forever),
            "ping_pong" => Ok(Repeat::PingPong),
            name => Err(format!("unknown behavior loops mode `{name}`")),
        },
    }
}

/// `retarget = "blend" | "restart"`: whether a write to a moving property
/// carries its speed into the new motion (`"blend"`, the default) or starts
/// the animation over from where the property is (`"restart"`). True for
/// blend.
pub fn retarget(name: Option<&str>) -> Result<bool, String> {
    match name {
        None | Some("blend") => Ok(true),
        Some("restart") => Ok(false),
        Some(name) => Err(format!(
            "behavior retarget must be \"blend\" or \"restart\", not `{name}`"
        )),
    }
}

/// `space = "srgb" | "oklab" | "oklch"`: where a colour travels.
pub fn color_space(name: Option<&str>) -> Result<ColorSpace, String> {
    match name {
        None => Ok(ColorSpace::default()),
        Some(name) => {
            ColorSpace::parse(name).ok_or_else(|| "space must be srgb, oklab or oklch".to_owned())
        }
    }
}

/// `hue = "shorter" | "longer"`: which way round the wheel in `oklch`.
pub fn hue(name: Option<&str>) -> Result<HueDirection, String> {
    match name {
        None => Ok(HueDirection::default()),
        Some(name) => {
            HueDirection::parse(name).ok_or_else(|| "hue must be shorter or longer".to_owned())
        }
    }
}

/// `rotation_direction`: which way an angle travels.
pub fn rotation_direction(name: Option<&str>) -> Result<RotationDirection, String> {
    match name {
        None | Some("numerical") => Ok(RotationDirection::Numerical),
        Some("shortest") => Ok(RotationDirection::Shortest),
        Some("clockwise") => Ok(RotationDirection::Clockwise),
        Some("counterclockwise") => Ok(RotationDirection::CounterClockwise),
        Some(_) => Err(
            "rotation_direction must be numerical, shortest, clockwise, or counterclockwise"
                .to_owned(),
        ),
    }
}
