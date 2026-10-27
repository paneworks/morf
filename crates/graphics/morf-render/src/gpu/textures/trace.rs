//! Opt-in drill-down of the image stage; source payloads are never printed.
use std::time::Instant;

pub(super) struct ImageTrace<'a> {
    started: Option<Instant>,
    last: Option<Instant>,
    source: &'a str,
    command: usize,
    pixels: Option<(u32, u32)>,
    stages: Vec<(&'static str, f64)>,
}
impl<'a> ImageTrace<'a> {
    pub(super) fn start(source: &'a str, command: usize) -> Self {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let on = *ON.get_or_init(|| morf_value::env::flags("MORF_PROFILE").has("render_split"));
        let started = on.then(Instant::now);
        Self {
            started,
            last: started,
            source,
            command,
            pixels: None,
            stages: Vec::new(),
        }
    }
    pub(super) fn mark(&mut self, stage: &'static str) {
        if let Some(last) = self.last.as_mut() {
            let now = Instant::now();
            self.stages
                .push((stage, (now - *last).as_secs_f64() * 1000.0));
            *last = now;
        }
    }
    pub(super) fn pixels(&mut self, width: u32, height: u32) {
        self.pixels = Some((width, height));
    }
}
impl Drop for ImageTrace<'_> {
    fn drop(&mut self) {
        let Some(started) = self.started else {
            return;
        };
        let total = started.elapsed().as_secs_f64() * 1000.0;
        static THRESHOLD: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
        let threshold = *THRESHOLD.get_or_init(|| {
            morf_value::env::flags("MORF_PROFILE")
                .parsed::<f64>("render_split")
                .filter(|value| value.is_finite() && *value >= 0.0)
                .unwrap_or(50.0)
        });
        if total < threshold {
            return;
        }
        let source =
            if self.source.starts_with("data:") || self.source.trim_start().starts_with('<') {
                format!("<inline image, {} bytes>", self.source.len())
            } else {
                self.source.chars().take(96).collect()
            };
        let parts = self
            .stages
            .iter()
            .map(|(stage, ms)| format!("{stage} {ms:.1}"))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "image command {} source {source:?} pixels {:?}: {total:.1} ms ({parts})",
            self.command, self.pixels
        );
    }
}
