//! Tests for the beat tracker: clicks at known tempos, noise, silence, and cost.

use super::*;

const RATE: u32 = 48_000;

/// A short decaying click every beat of `bpm`, for `seconds`, over
/// `noise` of white noise (a fixed generator, so every run is alike).
fn clicks(bpm: f32, seconds: f32, noise: f32, seed: &mut u32) -> Vec<f32> {
    let total = (seconds * RATE as f32) as usize;
    let period = 60.0 / bpm * RATE as f32;
    let mut out = Vec::with_capacity(total * 2);
    let mut next = 0.0f32;
    let mut click_at = usize::MAX;
    for index in 0..total {
        if index as f32 >= next {
            click_at = index;
            next += period;
        }
        let since = index.wrapping_sub(click_at);
        let click = if since < 2_000 {
            let t = since as f32 / RATE as f32;
            (2.0 * PI * 1_000.0 * t).sin() * (-t * 300.0).exp() * 0.8
        } else {
            0.0
        };
        let sample = click + noise * white(seed);
        out.push(sample);
        out.push(sample);
    }
    out
}

fn white(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32 / u32::MAX as f32) * 2.0 - 1.0
}

struct Heard {
    beats: Vec<f32>,
    tempo: Option<(f32, f32)>,
}

fn listen(tracker: &mut BeatTracker, samples: &[f32]) -> Heard {
    let mut heard = Heard {
        beats: Vec::new(),
        tempo: None,
    };
    // In buffers the size PipeWire hands over.
    for chunk in samples.chunks(1024 * 2) {
        tracker.push(chunk, 2);
        for event in tracker.drain() {
            match event {
                BeatEvent::Beat { strength } => heard.beats.push(strength),
                BeatEvent::Tempo { bpm, confidence } => heard.tempo = Some((bpm, confidence)),
            }
        }
    }
    heard
}

#[test]
fn clicks_at_120_bpm_are_beats_at_120_bpm() {
    let mut seed = 7;
    let mut tracker = BeatTracker::new(RATE);
    let heard = listen(&mut tracker, &clicks(120.0, 10.0, 0.0, &mut seed));
    // Twenty clicks; the first comes before the threshold has history.
    assert!(
        (18..=20).contains(&heard.beats.len()),
        "{} beats",
        heard.beats.len()
    );
    assert!(heard.beats.iter().all(|&s| s > 0.5), "{:?}", heard.beats);
    let (bpm, confidence) = heard.tempo.expect("a tempo");
    assert!((bpm - 120.0).abs() < 2.0, "{bpm} BPM");
    assert!(confidence > 0.5, "confidence {confidence}");
}

#[test]
fn clicks_over_noise_are_still_found() {
    let mut seed = 11;
    let mut tracker = BeatTracker::new(RATE);
    let heard = listen(&mut tracker, &clicks(100.0, 12.0, 0.05, &mut seed));
    assert!(
        (17..=21).contains(&heard.beats.len()),
        "{} beats",
        heard.beats.len()
    );
    let (bpm, _) = heard.tempo.expect("a tempo");
    assert!((bpm - 100.0).abs() < 2.0, "{bpm} BPM");
}

#[test]
fn noise_and_silence_make_no_beats() {
    let mut seed = 3;
    let mut tracker = BeatTracker::new(RATE);
    let noise: Vec<f32> = (0..RATE as usize * 2 * 10)
        .map(|_| 0.3 * white(&mut seed))
        .collect();
    let heard = listen(&mut tracker, &noise);
    assert!(
        heard.beats.len() <= 1,
        "{} beats in noise",
        heard.beats.len()
    );
    assert!(
        heard
            .tempo
            .is_none_or(|(_, confidence)| confidence < MIN_CONFIDENCE),
        "{:?}",
        heard.tempo
    );
    let mut tracker = BeatTracker::new(RATE);
    let heard = listen(&mut tracker, &vec![0.0; RATE as usize * 2 * 5]);
    assert!(heard.beats.is_empty() && heard.tempo.is_none());
}

#[test]
fn a_tempo_change_is_followed() {
    let mut seed = 5;
    let mut tracker = BeatTracker::new(RATE);
    let first = listen(&mut tracker, &clicks(120.0, 10.0, 0.0, &mut seed));
    assert!((first.tempo.unwrap().0 - 120.0).abs() < 2.0);
    let second = listen(&mut tracker, &clicks(90.0, 12.0, 0.0, &mut seed));
    let (bpm, _) = second.tempo.unwrap();
    assert!((bpm - 90.0).abs() < 2.0, "{bpm} BPM after the change");
}

#[test]
fn other_rates_and_mono_work_too() {
    let mut seed = 9;
    let stereo = clicks(128.0, 10.0, 0.0, &mut seed);
    // Every other sample: mono. Resampled to 44.1 kHz by nearest pick.
    let mono: Vec<f32> = stereo.iter().step_by(2).copied().collect();
    let resampled: Vec<f32> = (0..(mono.len() as f32 * 44_100.0 / 48_000.0) as usize)
        .map(|index| mono[(index as f32 * 48_000.0 / 44_100.0) as usize])
        .collect();
    let mut tracker = BeatTracker::new(44_100);
    let mut tempo = None;
    for chunk in resampled.chunks(1000) {
        tracker.push(chunk, 1);
        for event in tracker.drain() {
            if let BeatEvent::Tempo { bpm, .. } = event {
                tempo = Some(bpm);
            }
        }
    }
    let bpm = tempo.unwrap();
    assert!((bpm - 128.0).abs() < 2.5, "{bpm} BPM");
}

#[test]
fn tempos_across_the_range() {
    let mut misses = Vec::new();
    for bpm in (64..=196).step_by(6) {
        let mut seed = bpm as u32 + 1;
        let mut tracker = BeatTracker::new(RATE);
        let heard = listen(&mut tracker, &clicks(bpm as f32, 10.0, 0.02, &mut seed));
        let (found, confidence) = heard.tempo.unwrap_or((0.0, 0.0));
        if (found - bpm as f32).abs() > 1.5 || confidence < 0.5 {
            misses.push((bpm, found));
        }
    }
    assert!(misses.is_empty(), "{misses:?}");
}

#[test]
fn it_costs_a_small_fraction_of_real_time() {
    let mut seed = 13;
    let track = clicks(120.0, 30.0, 0.05, &mut seed);
    let mut tracker = BeatTracker::new(RATE);
    let start = std::time::Instant::now();
    for chunk in track.chunks(1024 * 2) {
        tracker.push(chunk, 2);
        tracker.drain().for_each(drop);
    }
    let spent = start.elapsed().as_secs_f32();
    eprintln!(
        "beat tracking: 30 s of audio in {:.1} ms ({:.2}% of real time)",
        spent * 1000.0,
        spent / 30.0 * 100.0
    );
    // Generous: a debug build on a busy machine.
    assert!(spent < if cfg!(debug_assertions) { 15.0 } else { 1.5 });
}
