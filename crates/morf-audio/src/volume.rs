//! Volume as a person means it, and volume as a sound server stores it.
//!
//! A server keeps one linear gain per channel: 1.0 passes the signal through,
//! 0.5 halves its amplitude. Nobody hears it that way — half the amplitude is
//! "a little quieter", not "half as loud" — so every mixer since PulseAudio
//! shows the cube root of the gain instead, and so does this crate. A slider
//! at 0.5 is the slider every other mixer on the desktop would show at 50%.

/// The loudest a volume may be set: 150%, the ceiling mixers allow for a
/// quiet source.
pub const MAX_VOLUME: f32 = 1.5;

/// A linear gain as the volume a person sees.
pub fn from_linear(gain: f32) -> f32 {
    if gain.is_finite() && gain > 0.0 {
        gain.cbrt()
    } else {
        0.0
    }
}

/// A volume a person chose as the linear gain a server applies.
pub fn to_linear(volume: f32) -> f32 {
    let volume = clamp(volume);
    volume * volume * volume
}

/// A volume held to what a server will take: finite, 0 to [`MAX_VOLUME`].
pub fn clamp(volume: f32) -> f32 {
    if volume.is_finite() {
        volume.clamp(0.0, MAX_VOLUME)
    } else {
        0.0
    }
}

/// The volume of several channels: the average of what each one shows.
pub fn average(gains: &[f32]) -> f32 {
    if gains.is_empty() {
        return 0.0;
    }
    gains.iter().map(|gain| from_linear(*gain)).sum::<f32>() / gains.len() as f32
}

/// New gains for every channel so their [`average`] becomes `volume`, keeping
/// the balance between them: a device panned left stays panned left.
///
/// Channels that were all silent come back level, since silence has no
/// balance to keep. An empty list means the server did not say how many
/// channels there are, and one gain is the only honest answer.
pub fn scale(gains: &[f32], volume: f32) -> Vec<f32> {
    let volume = clamp(volume);
    if gains.is_empty() {
        return vec![to_linear(volume)];
    }
    let current = average(gains);
    if current <= f32::EPSILON {
        return vec![to_linear(volume); gains.len()];
    }
    gains
        .iter()
        .map(|gain| to_linear(from_linear(*gain) * volume / current))
        .collect()
}
