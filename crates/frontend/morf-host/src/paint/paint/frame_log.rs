//! `MORF_FRAME_LOG`: how long frames take on the CPU side, and where a slow
//! one's time went.

pub fn frame_log_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED
        .get_or_init(|| std::env::var_os("MORF_FRAME_LOG").is_some_and(|value| !value.is_empty()))
}

/// Whether `MORF_FRAME_LOG=2` asks for each frame's detail.
pub fn frame_split_wanted() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("MORF_FRAME_LOG").is_ok_and(|value| value == "2"))
}

/// `MORF_FRAME_LOG=2`: where one frame's time went, stage by stage, for any
/// frame over 16 ms, or over `MORF_FRAME_SPLIT_MS` when that is set.
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
            std::env::var("MORF_FRAME_SPLIT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
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
