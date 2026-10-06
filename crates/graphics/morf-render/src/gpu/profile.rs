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

use super::targets::intersect_damage;
use crate::DamageRect;
use crate::effects::physical_damage;

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
        if std::env::var_os("MORF_GPU_PROFILE_COMMANDS").is_some()
            && adapter
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES)
        {
            needed | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES
        } else {
            needed
        }
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
    detailed: bool,
    draws: std::sync::Mutex<Vec<usize>>,
}

impl GpuProfile {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        let needed =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        if !wanted() || !device.features().contains(needed) {
            return None;
        }
        let detailed = device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES);
        let count = if detailed { 4096 } else { MARKS.len() as u32 };
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
            detailed,
            draws: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// Writes timestamp `index` (into `MARKS`) where the encoder has got to.
    pub(crate) fn mark(&self, encoder: &mut wgpu::CommandEncoder, index: usize) {
        if index == 0 {
            self.draws.lock().unwrap().clear();
        }
        encoder.write_timestamp(&self.queries, index as u32);
    }

    pub(crate) fn begin_draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        command: usize,
    ) -> Option<u32> {
        if !self.detailed {
            return None;
        }
        let mut draws = self.draws.lock().unwrap();
        let index = MARKS.len() as u32 + draws.len() as u32 * 2;
        if index + 1 >= 4096 {
            return None;
        }
        draws.push(command);
        pass.write_timestamp(&self.queries, index);
        Some(index + 1)
    }

    pub(crate) fn end_draw(&self, pass: &mut wgpu::RenderPass<'_>, index: u32) {
        pass.write_timestamp(&self.queries, index);
    }

    /// Copies the timestamps out, at the end of the frame's encoder.
    pub(crate) fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        let count = MARKS.len() as u32 + self.draws.lock().unwrap().len() as u32 * 2;
        encoder.resolve_query_set(&self.queries, 0..count, &self.resolved, 0);
        encoder.copy_buffer_to_buffer(&self.resolved, 0, &self.readback, 0, u64::from(count) * 8);
    }

    /// Waits for the frame and prints what each stage took, and what the
    /// damage made the commands shade.
    pub(crate) fn report(&self, device: &wgpu::Device, shading: &Shading) {
        let Shading {
            pixels,
            shaded,
            commands,
            heaviest,
        } = shading;
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
        let total =
            stamps[MARKS.len() - 1].saturating_sub(stamps[0]) as f64 * f64::from(self.period) / 1e6;
        eprintln!(
            "{} gpu profile {total:.2} ms for {pixels} damaged px: {}; {commands} commands shade {shaded} px ({:.1}x), most {heaviest}",
            super::present::stamp(),
            parts.join(", "),
            *shaded as f64 / (*pixels).max(1) as f64,
        );
        if self.detailed {
            let mut times = std::collections::BTreeMap::<usize, f64>::new();
            for (index, command) in self.draws.lock().unwrap().iter().enumerate() {
                let first = MARKS.len() + index * 2;
                *times.entry(*command).or_default() +=
                    stamps[first + 1].saturating_sub(stamps[first]) as f64 * f64::from(self.period)
                        / 1e6;
            }
            let mut times: Vec<_> = times.into_iter().collect();
            times.sort_by(|a, b| b.1.total_cmp(&a.1));
            eprintln!(
                "gpu commands: {}",
                times
                    .iter()
                    .take(8)
                    .map(|(id, ms)| format!("#{id} {ms:.2} ms"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}

/// What a frame's damage made its commands shade: each command's reach, cut
/// to its clip, over every damage rectangle. An estimate -- a large field
/// skips the tiles it cannot reach -- but it names what a frame is paying
/// for.
pub(crate) struct Shading {
    /// Damaged pixels.
    pixels: u64,
    /// Pixels the commands the damage reaches cover, together.
    shaded: u64,
    /// How many commands the damage reaches.
    commands: usize,
    /// The four that shade the most.
    heaviest: String,
}

impl Shading {
    pub(crate) fn of(
        list: &crate::DrawList,
        damage: &[DamageRect],
        reach: &[Option<DamageRect>],
        layered: impl Fn(usize) -> bool,
        scale_120: u32,
    ) -> Self {
        let area = |rect: DamageRect| u64::from(rect.width) * u64::from(rect.height);
        let mut shaded: Vec<(u64, usize)> = list
            .commands
            .iter()
            .enumerate()
            .map(|(index, command)| {
                let clip = command.clip().map(|clip| physical_damage(clip, scale_120));
                let total = damage
                    .iter()
                    .filter_map(|rect| {
                        let hit = intersect_damage(*rect, reach[index]?)?;
                        match clip {
                            Some(clip) => intersect_damage(hit, clip?),
                            None => Some(hit),
                        }
                    })
                    .map(area)
                    .sum::<u64>();
                (total, index)
            })
            .filter(|(total, _)| *total > 0)
            .collect();
        shaded.sort_unstable_by(|left, right| right.cmp(left));
        let heaviest = shaded
            .iter()
            .take(4)
            .map(|(total, index)| {
                let name = format!("{:?}", list.commands[*index]);
                let kind = name.split([' ', '{', '(']).next().unwrap_or("?");
                let inside = if layered(*index) { " in a layer" } else { "" };
                let detail = match &list.commands[*index] {
                    crate::DrawCommand::Field {
                        layers,
                        gradient,
                        shadow_color,
                        ..
                    } => {
                        let shapes: Vec<_> = layers
                            .iter()
                            .filter(|l| l.opacity > 0.0)
                            .map(|l| format!("{:?}:{:.2}", l.shape, l.opacity))
                            .collect();
                        format!(
                            "({:?},[{}],gradient={},shadow={})",
                            super::field_pass::FieldVariant::for_command(&list.commands[*index]),
                            shapes.join("/"),
                            gradient.is_some(),
                            shadow_color.alpha > 0.0
                        )
                    }
                    crate::DrawCommand::Quad {
                        gradient,
                        shadow_color,
                        ..
                    } => format!(
                        "(gradient={},shadow={})",
                        gradient.is_some(),
                        shadow_color.alpha > 0.0
                    ),
                    _ => String::new(),
                };
                format!("{kind}#{index}{inside} {total}{detail}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        Self {
            pixels: damage.iter().map(|rect| area(*rect)).sum(),
            shaded: shaded.iter().map(|(total, _)| total).sum(),
            commands: shaded.len(),
            heaviest,
        }
    }
}
