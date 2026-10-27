//! `MORF_PROFILE=render_split`: where a slow frame's render went on the CPU, step by
//! step -- the fields, the glyphs, the images, the layers, the upload, the
//! encoding, and acquiring, submitting and presenting the frame.

use std::time::Instant;

pub(super) struct RenderSplit {
    on: bool,
    started: Instant,
    last: Instant,
    stages: Vec<(&'static str, f64)>,
}

impl RenderSplit {
    pub(super) fn start() -> Self {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let on = *ON.get_or_init(|| morf_value::env::flags("MORF_PROFILE").has("render_split"));
        let now = Instant::now();
        Self {
            on,
            started: now,
            last: now,
            stages: Vec::new(),
        }
    }

    pub(super) fn mark(&mut self, stage: &'static str) {
        if !self.on {
            return;
        }
        let now = Instant::now();
        self.stages
            .push((stage, (now - self.last).as_secs_f64() * 1000.0));
        self.last = now;
    }

    /// Says where the time went, for a render of 50 ms or more, or the
    /// threshold in `MORF_PROFILE=render_split` when measuring shorter frames.
    pub(super) fn finish(self) {
        static THRESHOLD: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
        let threshold = *THRESHOLD.get_or_init(|| {
            morf_value::env::flags("MORF_PROFILE")
                .parsed::<f64>("render_split")
                .filter(|value| value.is_finite() && *value >= 0.0)
                .unwrap_or(50.0)
        });
        let total = self.started.elapsed().as_secs_f64() * 1000.0;
        if !self.on || total < threshold {
            return;
        }
        let parts = self
            .stages
            .iter()
            .map(|(stage, ms)| format!("{stage} {ms:.1}"))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!("render split {total:.1} ms: {parts}");
    }
}
