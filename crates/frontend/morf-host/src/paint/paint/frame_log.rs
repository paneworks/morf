//! `MORF_LOG=frame`: how long frames take on the CPU side, and where a slow
//! one's time went.

pub fn frame_log_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| morf_value::env::log("frame"))
}

/// Whether `MORF_PROFILE=frame_split` asks for each frame's detail.
pub fn frame_split_wanted() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| morf_value::env::flags("MORF_PROFILE").has("frame_split"))
}

/// `MORF_PROFILE=frame_split`: where one frame's time went, stage by stage, for
/// any frame over 16 ms, or over N with `MORF_PROFILE=frame_split=N`.
pub(super) struct FrameSplit {
    pub(super) on: bool,
    started: std::time::Instant,
    last: std::time::Instant,
    stages: Vec<(&'static str, f64)>,
}

impl FrameSplit {
    pub(super) fn start() -> Self {
        let on = frame_split_wanted();
        let now = std::time::Instant::now();
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
        let now = std::time::Instant::now();
        self.stages
            .push((stage, (now - self.last).as_secs_f64() * 1000.0));
        self.last = now;
    }

    pub(super) fn finish(self) {
        static THRESHOLD: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
        let threshold = *THRESHOLD.get_or_init(|| {
            morf_value::env::flags("MORF_PROFILE")
                .parsed("frame_split")
                .unwrap_or(16.0)
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
        eprintln!(
            "{} frame split {total:.1} ms: {parts}",
            crate::paint::wake_plan::stamp()
        );
    }
}
