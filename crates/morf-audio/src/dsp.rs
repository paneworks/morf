//! Turning samples into something a meter can draw.
//!
//! Plain arithmetic on interleaved `f32` frames, with no idea where they came
//! from, so any backend that can hand over samples gets the same meters: the
//! loudest sample per side over a window, and optionally the energy in a
//! number of frequency bands spaced the way hearing spaces them.

use std::f32::consts::PI;

/// Samples the band analyser looks at: 2048 frames is about 43 ms at
/// 48 kHz, enough to tell 30 Hz from 50 Hz.
const FFT_SIZE: usize = 2048;
/// The lowest and highest frequencies the bands cover.
const LOW_HZ: f32 = 50.0;
const HIGH_HZ: f32 = 16_000.0;
/// The quietest a band can be and still show: -60 dB below full scale.
const FLOOR_DB: f32 = -60.0;
/// The most bands one monitor may ask for.
pub const MAX_BANDS: usize = 128;

/// One reading of a [`Meter`].
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub left: f32,
    pub right: f32,
    pub bands: Vec<f32>,
}

/// Peaks per side, and band energies when asked, emitted at a steady rate.
pub struct Meter {
    rate_hz: f32,
    rate: u32,
    channels: usize,
    window: usize,
    counted: usize,
    peak: [f32; 2],
    bands: Option<Bands>,
}

impl Meter {
    /// A meter reading `rate_hz` times a second, with `bands` frequency
    /// bands (none when 0). The sample rate defaults to 48 kHz stereo until
    /// [`Meter::set_format`] says otherwise.
    pub fn new(rate_hz: f32, bands: usize) -> Self {
        let rate_hz = if rate_hz.is_finite() {
            rate_hz.clamp(1.0, 120.0)
        } else {
            30.0
        };
        let bands = bands.min(MAX_BANDS);
        let mut meter = Self {
            rate_hz,
            rate: 48_000,
            channels: 2,
            window: 1,
            counted: 0,
            peak: [0.0; 2],
            bands: (bands > 0).then(|| Bands::new(bands, 48_000)),
        };
        meter.set_format(48_000, 2);
        meter
    }

    /// The format the samples arrive in.
    pub fn set_format(&mut self, rate: u32, channels: u32) {
        let rate = rate.max(1);
        self.rate = rate;
        self.channels = channels.max(1) as usize;
        self.window = ((rate as f32 / self.rate_hz) as usize).max(1);
        if let Some(bands) = &mut self.bands {
            *bands = Bands::new(bands.count, rate);
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Forgets what it heard and reads as silence — for when samples stop
    /// coming (the device went idle), so a meter falls to zero rather than
    /// holding its last reading forever.
    pub fn silence(&mut self) -> Reading {
        self.counted = 0;
        self.peak = [0.0; 2];
        let bands = match &mut self.bands {
            Some(bands) => {
                bands.ring.fill(0.0);
                vec![0.0; bands.count]
            }
            None => Vec::new(),
        };
        Reading {
            left: 0.0,
            right: 0.0,
            bands,
        }
    }

    /// Takes interleaved frames; returns a reading when a window is full.
    /// A buffer longer than a window still makes one reading, of all of it:
    /// the caller wants the latest, not a backlog.
    pub fn push(&mut self, samples: &[f32]) -> Option<Reading> {
        let channels = self.channels;
        let mut frames = 0;
        for frame in samples.chunks_exact(channels) {
            let left = sanitize(frame[0]);
            let right = if channels > 1 {
                sanitize(frame[1])
            } else {
                left
            };
            self.peak[0] = self.peak[0].max(left.abs());
            self.peak[1] = self.peak[1].max(right.abs());
            if let Some(bands) = &mut self.bands {
                bands.push((left + right) * 0.5);
            }
            frames += 1;
        }
        self.counted += frames;
        if self.counted < self.window {
            return None;
        }
        self.counted = 0;
        let reading = Reading {
            left: self.peak[0].min(1.0),
            right: self.peak[1].min(1.0),
            bands: self.bands.as_mut().map(Bands::analyse).unwrap_or_default(),
        };
        self.peak = [0.0; 2];
        Some(reading)
    }
}

fn sanitize(sample: f32) -> f32 {
    if sample.is_finite() { sample } else { 0.0 }
}

/// Band energies from the most recent [`FFT_SIZE`] samples.
struct Bands {
    count: usize,
    ring: Vec<f32>,
    at: usize,
    window: Vec<f32>,
    /// The FFT bins each band spans, as a half-open range.
    ranges: Vec<(usize, usize)>,
    fft: Fft,
}

impl Bands {
    fn new(count: usize, rate: u32) -> Self {
        let window = (0..FFT_SIZE)
            .map(|index| 0.5 - 0.5 * (2.0 * PI * index as f32 / FFT_SIZE as f32).cos())
            .collect();
        Self {
            count,
            ring: vec![0.0; FFT_SIZE],
            at: 0,
            window,
            ranges: band_ranges(count, rate, FFT_SIZE),
            fft: Fft::new(FFT_SIZE),
        }
    }

    fn push(&mut self, sample: f32) {
        self.ring[self.at] = sample;
        self.at = (self.at + 1) % FFT_SIZE;
    }

    fn analyse(&mut self) -> Vec<f32> {
        let mut buffer: Vec<(f32, f32)> = (0..FFT_SIZE)
            .map(|index| {
                (
                    self.ring[(self.at + index) % FFT_SIZE] * self.window[index],
                    0.0,
                )
            })
            .collect();
        self.fft.run(&mut buffer);
        // A full-scale sine through a Hann window peaks at N/4.
        let full_scale = FFT_SIZE as f32 / 4.0;
        self.ranges
            .iter()
            .map(|&(start, end)| {
                let magnitude = buffer[start..end]
                    .iter()
                    .map(|(re, im)| (re * re + im * im).sqrt())
                    .fold(0.0f32, f32::max);
                let db = 20.0 * (magnitude / full_scale).max(1e-9).log10();
                ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
            })
            .collect()
    }
}

/// Which FFT bins each of `count` bands covers, spaced evenly on a log scale
/// between [`LOW_HZ`] and [`HIGH_HZ`] (or Nyquist, when that is lower). Every
/// band gets at least one bin, so narrow low bands repeat a bin rather than
/// read as silent.
pub(crate) fn band_ranges(count: usize, rate: u32, size: usize) -> Vec<(usize, usize)> {
    let nyquist = rate as f32 / 2.0;
    let high = HIGH_HZ.min(nyquist * 0.95);
    let low = LOW_HZ.min(high / 2.0);
    let bin_hz = rate as f32 / size as f32;
    let last = size / 2;
    (0..count)
        .map(|band| {
            let from = low * (high / low).powf(band as f32 / count as f32);
            let to = low * (high / low).powf((band + 1) as f32 / count as f32);
            let start = ((from / bin_hz).round() as usize).clamp(1, last - 1);
            let end = ((to / bin_hz).round() as usize).clamp(start + 1, last);
            (start, end)
        })
        .collect()
}

/// An in-place radix-2 FFT of one fixed size.
struct Fft {
    size: usize,
    twiddles: Vec<(f32, f32)>,
}

impl Fft {
    fn new(size: usize) -> Self {
        debug_assert!(size.is_power_of_two());
        let twiddles = (0..size / 2)
            .map(|index| {
                let angle = -2.0 * PI * index as f32 / size as f32;
                (angle.cos(), angle.sin())
            })
            .collect();
        Self { size, twiddles }
    }

    fn run(&self, data: &mut [(f32, f32)]) {
        let n = self.size;
        let bits = n.trailing_zeros();
        for index in 0..n {
            let reversed = index.reverse_bits() >> (usize::BITS - bits);
            if reversed > index {
                data.swap(index, reversed);
            }
        }
        let mut length = 2;
        while length <= n {
            let stride = n / length;
            for start in (0..n).step_by(length) {
                for offset in 0..length / 2 {
                    let (wr, wi) = self.twiddles[offset * stride];
                    let (ar, ai) = data[start + offset];
                    let (br, bi) = data[start + offset + length / 2];
                    let (tr, ti) = (br * wr - bi * wi, br * wi + bi * wr);
                    data[start + offset] = (ar + tr, ai + ti);
                    data[start + offset + length / 2] = (ar - tr, ai - ti);
                }
            }
            length *= 2;
        }
    }
}
