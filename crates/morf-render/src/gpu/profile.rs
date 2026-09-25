//! `MORF_GPU_PROFILE=1`: where a frame's GPU time goes.
//!
//! `MORF_GPU_WAIT` says how long a frame took on the GPU, which on a GPU the
//! compositor is keeping busy is mostly time spent queued behind it. This
//! says what the frame's own work cost, stage by stage, from timestamps the
//! GPU writes as it gets there: the offscreen layers (with their blurs and
//! backdrops), the surface pass, and the copy onto what is presented.
//!
//! Each frame waits for its timestamps, so it is for measuring only. On a
//! device without timestamp queries inside encoders it prints nothing.

/// The stages timestamps are written between.
pub(crate) const MARKS: [&str; 4] = ["start", "layers", "surface", "present"];

/// Whether profiling was asked for.
pub(crate) fn wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| std::env::var_os("MORF_GPU_PROFILE").is_some())
}

/// The device features profiling needs, when it is asked for and the adapter
/// has them.
pub(crate) fn features(adapter: &wgpu::Adapter) -> wgpu::Features {
    let needed = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
    if wanted() && adapter.features().contains(needed) {
        needed
    } else {
        wgpu::Features::empty()
    }
}

/// A frame's timestamps, and the buffers they are read back through.
pub(crate) struct GpuProfile {
    queries: wgpu::QuerySet,
    resolved: wgpu::Buffer,
    readback: wgpu::Buffer,
    period: f32,
}

impl GpuProfile {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        let needed =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        if !wanted() || !device.features().contains(needed) {
            return None;
        }
        let count = MARKS.len() as u32;
        let size = u64::from(count) * 8;
        Some(Self {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("morf gpu profile"),
                ty: wgpu::QueryType::Timestamp,
                count,
            }),
            resolved: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("morf gpu profile resolved"),
                size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("morf gpu profile readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            period: queue.get_timestamp_period(),
        })
    }

    /// Writes timestamp `index` (into `MARKS`) where the encoder has got to.
    pub(crate) fn mark(&self, encoder: &mut wgpu::CommandEncoder, index: usize) {
        encoder.write_timestamp(&self.queries, index as u32);
    }

    /// Copies the timestamps out, at the end of the frame's encoder.
    pub(crate) fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.resolve_query_set(&self.queries, 0..MARKS.len() as u32, &self.resolved, 0);
        encoder.copy_buffer_to_buffer(&self.resolved, 0, &self.readback, 0, self.resolved.size());
    }

    /// Waits for the frame and prints what each stage took. `shaded` is how
    /// many pixels the commands the damage reaches cover, and how many
    /// commands; `heaviest` names the largest.
    pub(crate) fn report(
        &self,
        device: &wgpu::Device,
        pixels: u64,
        (shaded, commands): (u64, usize),
        heaviest: &str,
    ) {
        let slice = self.readback.slice(..);
        let (send, receive) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        if !matches!(receive.recv(), Ok(Ok(()))) {
            return;
        }
        let stamps: Vec<u64> = {
            let Ok(mapped) = slice.get_mapped_range() else {
                return;
            };
            bytemuck::cast_slice::<u8, u64>(&mapped).to_vec()
        };
        self.readback.unmap();
        let parts: Vec<String> = stamps
            .windows(2)
            .zip(&MARKS[1..])
            .map(|(pair, name)| {
                let ms = pair[1].saturating_sub(pair[0]) as f64 * f64::from(self.period) / 1e6;
                format!("{name} {ms:.2}")
            })
            .collect();
        let total = stamps.last().unwrap_or(&0).saturating_sub(stamps[0]) as f64
            * f64::from(self.period)
            / 1e6;
        eprintln!(
            "{} gpu profile {total:.2} ms for {pixels} damaged px: {}; {commands} commands shade {shaded} px ({:.1}x), most {heaviest}",
            super::present::stamp(),
            parts.join(", "),
            shaded as f64 / pixels.max(1) as f64,
        );
    }
}
