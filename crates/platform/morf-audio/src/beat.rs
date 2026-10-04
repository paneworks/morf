//! Beats and tempo from a stream of samples.
//!
//! Onsets are found the usual way: the spectrum of each hop of samples is
//! compared with the one before, and the rise in (log-compressed)
//! magnitude summed over the bins — the *spectral flux* — spikes where
//! something starts. A spike that is a local peak and stands clear of a
//! threshold that follows the recent flux (its mean plus some of its
//! spread, and a margin over the mean) is a beat; its strength is its size
//! against the loudest recent beat. Steady sound, however loud — a drone,
//! noise — has a flat flux and makes none.
//!
//! The tempo is read off the flux itself: its autocorrelation over the
//! last few seconds peaks at the lag between beats. Lags between 60 and
//! 200 BPM are weighed with their double (so the beat wins over the bar)
//! and a mild preference for moderate tempos, the peak is interpolated
//! between lags, and the estimate moves only when readings agree: close to
//! the current one they are smoothed in, far from it they must agree with
//! each other several times running before the estimate jumps. The
//! confidence is how strongly the flux repeats at that lag, 0 to 1.

use std::f32::consts::PI;

use crate::dsp::Fft;

/// Samples per spectrum, and between spectra.
const WINDOW: usize = 1024;
const HOP: usize = 512;
/// How much of the flux's recent history the threshold follows, seconds.
const THRESHOLD_SECONDS: f32 = 0.5;
/// A beat's flux is at least this far over the mean, in standard deviations
/// and as a ratio.
const THRESHOLD_DEVIATIONS: f32 = 2.0;
const THRESHOLD_RATIO: f32 = 1.5;
/// Below this the flux is silence, whatever its shape.
const FLUX_FLOOR: f32 = 0.02;
/// No two beats closer than this, seconds.
const REFRACTORY_SECONDS: f32 = 0.1;
/// How fast the loudest beat is forgotten, per second.
const PEAK_DECAY_PER_SECOND: f32 = 0.5;
/// Log compression of the magnitudes: log(1 + C·|X|).
const COMPRESSION: f32 = 100.0;
/// Tempo range, and how much flux history the tempo is read from.
const MIN_BPM: f32 = 60.0;
const MAX_BPM: f32 = 200.0;
const TEMPO_SECONDS: f32 = 6.0;
/// The least history before a tempo is guessed, seconds, and how often one
/// is.
const TEMPO_MIN_SECONDS: f32 = 2.5;
const TEMPO_EVERY_SECONDS: f32 = 0.5;
/// Readings this close (as a fraction) are the same tempo.
const SAME_TEMPO: f32 = 0.04;
/// A far reading has to come this many times running to be believed.
const SWITCH_AFTER: u32 = 3;
/// How strongly the flux must repeat at half the lag found for the
/// tempo to be read as twice as fast.
const HALF_LAG_RATIO: f32 = 0.85;
/// Readings less confident than this are ignored.
const MIN_CONFIDENCE: f32 = 0.2;

/// What a [`BeatTracker`] noticed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BeatEvent {
    /// Something started; strength 0 to 1 against recent beats.
    Beat { strength: f32 },
    /// The tempo estimate, beats per minute, and how sure it is, 0 to 1.
    Tempo { bpm: f32, confidence: f32 },
}

/// Beat and tempo detection over mono samples at one sample rate.
pub struct BeatTracker {
    rate: f32,
    /// Spectra per second.
    fps: f32,
    ring: Vec<f32>,
    at: usize,
    since_hop: usize,
    hann: Vec<f32>,
    fft: Fft,
    buffer: Vec<(f32, f32)>,
    previous: Vec<f32>,
    current: Vec<f32>,
    primed: bool,
    /// Recent flux, for the threshold; the latest last.
    recent: Vec<f32>,
    recent_at: usize,
    recent_len: usize,
    /// The last two flux values and the threshold the middle one faced, for
    /// peak picking one hop late.
    before: f32,
    candidate: f32,
    candidate_threshold: f32,
    hops_since_beat: usize,
    refractory: usize,
    peak: f32,
    peak_decay: f32,
    /// The onset envelope the tempo is read from.
    envelope: Vec<f32>,
    envelope_at: usize,
    envelope_len: usize,
    hops_since_tempo: usize,
    tempo_every: usize,
    tempo: Option<f32>,
    confidence: f32,
    challenger: Option<f32>,
    challenges: u32,
    events: Vec<BeatEvent>,
    scratch_raw: Vec<f32>,
    scratch_values: Vec<f32>,
    scratch_acf: Vec<f32>,
}

impl BeatTracker {
    pub fn new(rate: u32) -> Self {
        let rate = rate.max(8_000) as f32;
        let fps = rate / HOP as f32;
        let recent_capacity = ((THRESHOLD_SECONDS * fps) as usize).max(4);
        let envelope_capacity = (TEMPO_SECONDS * fps) as usize;
        Self {
            rate,
            fps,
            ring: vec![0.0; WINDOW],
            at: 0,
            since_hop: 0,
            hann: (0..WINDOW)
                .map(|index| 0.5 - 0.5 * (2.0 * PI * index as f32 / WINDOW as f32).cos())
                .collect(),
            fft: Fft::new(WINDOW),
            buffer: vec![(0.0, 0.0); WINDOW],
            previous: vec![0.0; WINDOW / 2],
            current: vec![0.0; WINDOW / 2],
            primed: false,
            recent: vec![0.0; recent_capacity],
            recent_at: 0,
            recent_len: 0,
            before: 0.0,
            candidate: 0.0,
            candidate_threshold: f32::INFINITY,
            hops_since_beat: usize::MAX / 2,
            refractory: ((REFRACTORY_SECONDS * fps) as usize).max(1),
            peak: 0.0,
            peak_decay: PEAK_DECAY_PER_SECOND.powf(1.0 / fps),
            envelope: vec![0.0; envelope_capacity],
            envelope_at: 0,
            envelope_len: 0,
            hops_since_tempo: 0,
            tempo_every: ((TEMPO_EVERY_SECONDS * fps) as usize).max(1),
            tempo: None,
            confidence: 0.0,
            challenger: None,
            challenges: 0,
            events: Vec::with_capacity(8),
            scratch_raw: Vec::new(),
            scratch_values: Vec::new(),
            scratch_acf: Vec::new(),
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate as u32
    }

    /// The tempo estimate and its confidence, once there is one.
    pub fn tempo(&self) -> Option<(f32, f32)> {
        self.tempo.map(|bpm| (bpm, self.confidence))
    }

    /// Takes interleaved frames of `channels` channels.
    pub fn push(&mut self, samples: &[f32], channels: usize) {
        let channels = channels.max(1);
        for frame in samples.chunks_exact(channels) {
            let left = finite(frame[0]);
            let mono = if channels > 1 {
                (left + finite(frame[1])) * 0.5
            } else {
                left
            };
            self.ring[self.at] = mono;
            self.at = (self.at + 1) % WINDOW;
            self.since_hop += 1;
            if self.since_hop == HOP {
                self.since_hop = 0;
                self.hop();
            }
        }
    }

    /// What was noticed since the last call, oldest first.
    pub fn drain(&mut self) -> std::vec::Drain<'_, BeatEvent> {
        self.events.drain(..)
    }

    /// The sound stopped: forget the onsets, keep the tempo (it will be
    /// back), and start comparing spectra afresh.
    pub fn silence(&mut self) {
        self.ring.fill(0.0);
        self.primed = false;
        self.recent_len = 0;
        self.envelope_len = 0;
        self.before = 0.0;
        self.candidate = 0.0;
        self.candidate_threshold = f32::INFINITY;
        self.confidence *= 0.5;
    }

    fn hop(&mut self) {
        for index in 0..WINDOW {
            self.buffer[index] = (
                self.ring[(self.at + index) % WINDOW] * self.hann[index],
                0.0,
            );
        }
        self.fft.run(&mut self.buffer);
        let full_scale = WINDOW as f32 / 4.0;
        for (bin, value) in self.current.iter_mut().enumerate() {
            let (re, im) = self.buffer[bin];
            let magnitude = (re * re + im * im).sqrt() / full_scale;
            *value = (COMPRESSION * magnitude).ln_1p();
        }
        let flux = if self.primed {
            let rise: f32 = self
                .current
                .iter()
                .zip(&self.previous)
                .skip(1)
                .map(|(now, before)| (now - before).max(0.0))
                .sum();
            rise / (WINDOW / 2 - 1) as f32
        } else {
            self.primed = true;
            0.0
        };
        std::mem::swap(&mut self.current, &mut self.previous);
        self.onset(flux);
        self.remember(flux);
    }

    /// Peak picking, one hop late: the previous flux is a beat when it
    /// beat its neighbours and its threshold.
    fn onset(&mut self, flux: f32) {
        self.hops_since_beat = self.hops_since_beat.saturating_add(1);
        self.peak *= self.peak_decay;
        let candidate = self.candidate;
        if candidate > self.before
            && candidate >= flux
            && candidate > self.candidate_threshold
            && self.hops_since_beat > self.refractory
        {
            self.peak = self.peak.max(candidate);
            self.hops_since_beat = 0;
            let strength = if self.peak > 0.0 {
                (candidate / self.peak).clamp(0.0, 1.0)
            } else {
                1.0
            };
            self.events.push(BeatEvent::Beat { strength });
        }
        self.before = candidate;
        self.candidate = flux;
        self.candidate_threshold = self.threshold();
    }

    /// What the next flux must clear, from the flux before it.
    fn threshold(&self) -> f32 {
        if self.recent_len < self.recent.len() / 2 {
            return f32::INFINITY;
        }
        let values = &self.recent[..self.recent_len];
        let count = values.len() as f32;
        let mean = values.iter().sum::<f32>() / count;
        let variance = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / count;
        (mean + THRESHOLD_DEVIATIONS * variance.sqrt())
            .max(mean * THRESHOLD_RATIO)
            .max(FLUX_FLOOR)
    }

    fn remember(&mut self, flux: f32) {
        let capacity = self.recent.len();
        self.recent[self.recent_at] = flux;
        self.recent_at = (self.recent_at + 1) % capacity;
        self.recent_len = (self.recent_len + 1).min(capacity);

        let capacity = self.envelope.len();
        self.envelope[self.envelope_at] = flux;
        self.envelope_at = (self.envelope_at + 1) % capacity;
        self.envelope_len = (self.envelope_len + 1).min(capacity);
        self.hops_since_tempo += 1;
        if self.hops_since_tempo >= self.tempo_every
            && self.envelope_len as f32 >= TEMPO_MIN_SECONDS * self.fps
        {
            self.hops_since_tempo = 0;
            self.estimate_tempo();
        }
    }

    fn estimate_tempo(&mut self) {
        // The envelope in order, oldest first, less its mean.
        let capacity = self.envelope.len();
        let length = self.envelope_len;
        let start = (self.envelope_at + capacity - length) % capacity;
        // Taken out of `self` for the length of the estimate, so the
        // audio thread allocates nothing after the first few.
        let mut raw = std::mem::take(&mut self.scratch_raw);
        let mut values = std::mem::take(&mut self.scratch_values);
        let mut acf = std::mem::take(&mut self.scratch_acf);
        raw.clear();
        raw.extend((0..length).map(|index| self.envelope[(start + index) % capacity]));
        // Smoothed a little: a beat whose period is not a whole number of
        // hops lands a hop early or late, and a spike one hop wide would
        // split its correlation between two lags.
        const KERNEL: [f32; 5] = [1.0, 2.0, 3.0, 2.0, 1.0];
        values.clear();
        values.extend((0..length).map(|index| {
            let mut sum = 0.0;
            let mut weight = 0.0;
            for (offset, k) in KERNEL.iter().enumerate() {
                if let Some(value) = (index + offset).checked_sub(2).and_then(|at| raw.get(at)) {
                    sum += k * value;
                    weight += k;
                }
            }
            sum / weight
        }));
        self.correlate(&mut values, &mut acf);
        self.scratch_raw = raw;
        self.scratch_values = values;
        self.scratch_acf = acf;
    }

    /// Reads a tempo off the smoothed envelope, into `acf` as scratch.
    fn correlate(&mut self, values: &mut [f32], acf: &mut Vec<f32>) {
        let length = values.len();
        let mean = values.iter().sum::<f32>() / length as f32;
        for value in values.iter_mut() {
            *value -= mean;
        }
        let energy = values.iter().map(|v| v * v).sum::<f32>() / length as f32;
        if energy <= 1e-12 {
            self.decay_confidence();
            return;
        }
        // Normalised, unbiased autocorrelation at a lag.
        let correlation = |lag: usize| -> f32 {
            if lag >= length {
                return 0.0;
            }
            let pairs = length - lag;
            let sum: f32 = values[..pairs]
                .iter()
                .zip(&values[lag..])
                .map(|(a, b)| a * b)
                .sum();
            sum / pairs as f32 / energy
        };
        let shortest = (60.0 * self.fps / MAX_BPM).floor().max(1.0) as usize;
        let longest = (60.0 * self.fps / MIN_BPM).ceil() as usize;
        if longest + 1 >= length {
            return;
        }
        acf.clear();
        acf.extend((0..=longest * 2 + 1).map(correlation));
        let acf = &acf[..];
        let score = |lag: usize| -> f32 {
            let bpm = 60.0 * self.fps / lag as f32;
            // A mild preference for moderate tempos: a log-Gaussian around
            // 120 BPM, an octave wide.
            let octaves = (bpm / 120.0).log2();
            let prior = (-0.5 * octaves * octaves).exp();
            let double = acf.get(lag * 2).copied().unwrap_or(0.0).max(0.0);
            (acf[lag].max(0.0) + 0.5 * double) * (0.5 + 0.5 * prior)
        };
        let Some(mut best) = (shortest..=longest).max_by(|&a, &b| score(a).total_cmp(&score(b)))
        else {
            return;
        };
        // A beat repeats at twice its period too, so the peak found may be
        // every other beat. When the flux repeats nearly as strongly at half
        // the lag, that is the beat.
        while best / 2 >= shortest {
            let half = best / 2;
            let Some(near) = (half.saturating_sub(1).max(shortest)..=(half + 1).min(longest))
                .max_by(|&a, &b| acf[a].total_cmp(&acf[b]))
            else {
                break;
            };
            if acf[near] >= HALF_LAG_RATIO * acf[best] {
                best = near;
            } else {
                break;
            }
        }
        // Interpolate the peak between lags.
        let lag = if best > shortest && best < longest {
            let (a, b, c) = (acf[best - 1], acf[best], acf[best + 1]);
            let curvature = a - 2.0 * b + c;
            if curvature < 0.0 {
                best as f32 + 0.5 * (a - c) / curvature
            } else {
                best as f32
            }
        } else {
            best as f32
        };
        let bpm = 60.0 * self.fps / lag;
        let confidence = acf[best].clamp(0.0, 1.0);
        self.update_tempo(bpm, confidence);
    }

    fn decay_confidence(&mut self) {
        self.confidence *= 0.8;
    }

    /// Moves the estimate, or not.
    fn update_tempo(&mut self, bpm: f32, confidence: f32) {
        if confidence < MIN_CONFIDENCE {
            self.decay_confidence();
            if let Some(tempo) = self.tempo {
                self.events.push(BeatEvent::Tempo {
                    bpm: tempo,
                    confidence: self.confidence,
                });
            }
            return;
        }
        let same = |a: f32, b: f32| (a - b).abs() <= SAME_TEMPO * b;
        match self.tempo {
            None => {
                self.tempo = Some(bpm);
                self.confidence = confidence;
            }
            Some(tempo) if same(bpm, tempo) => {
                self.tempo = Some(tempo + 0.3 * (bpm - tempo));
                self.confidence += 0.3 * (confidence - self.confidence);
                self.challenger = None;
                self.challenges = 0;
            }
            Some(_) => {
                match self.challenger {
                    Some(challenger) if same(bpm, challenger) => self.challenges += 1,
                    _ => self.challenges = 1,
                }
                self.challenger = Some(bpm);
                self.confidence *= 0.9;
                if self.challenges >= SWITCH_AFTER {
                    self.tempo = Some(bpm);
                    self.confidence = confidence;
                    self.challenger = None;
                    self.challenges = 0;
                }
            }
        }
        if let Some(tempo) = self.tempo {
            self.events.push(BeatEvent::Tempo {
                bpm: tempo,
                confidence: self.confidence,
            });
        }
    }
}

fn finite(sample: f32) -> f32 {
    if sample.is_finite() { sample } else { 0.0 }
}

#[cfg(test)]
mod tests;
