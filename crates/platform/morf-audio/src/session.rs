//! What a shell keeps of the audio between frames: the [`Audio`], started
//! the first time it is asked for, the handlers that hear about changes and
//! the monitors that meter devices.
//!
//! A [`Session`] names no scripting language. A handler is whatever the
//! shell calls one (`H`) and a data channel whatever it writes bands to
//! (`C`); [`Session::poll`] hands back which handler to call with which
//! values, and the rows to put in the shell's lists, for the shell to act on.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

use morf_value::{IpcTable, IpcValue};

use crate::spectrum::Filter;
use crate::{Audio, DeviceKind, MonitorDelay, ObjectId};
use crate::rows::{device_row, stream_row};

/// How many change handlers one shell may hold.
pub const MAX_LISTENERS: usize = 32;
/// How many monitors one shell may run.
pub const MAX_MONITORS: usize = 8;

/// What one monitor calls, and the tempo it last heard.
pub struct Monitor<H, C> {
    pub on_level: Option<H>,
    pub on_beat: Option<H>,
    pub on_tempo: Option<H>,
    pub tempo: Option<(f32, f32)>,
    /// Each reading's bands written straight to a data channel, through the
    /// `spectrum` filter when one is given -- no handler per frame.
    pub channel: Option<MonitorChannel<C>>,
}

/// A monitor's data channel and the filter its bands pass through.
pub struct MonitorChannel<C> {
    pub channel: C,
    pub filter: Option<Filter>,
    pub last: Option<Instant>,
}

impl<C> MonitorChannel<C> {
    pub fn new(channel: C, filter: Option<Filter>) -> Self {
        Self {
            channel,
            filter,
            last: None,
        }
    }

    /// The values a reading's bands become, filtered when there is a filter;
    /// none when the filter refuses them.
    fn step(&mut self, bands: &[f32]) -> Option<Vec<f32>> {
        let now = Instant::now();
        let dt = self.last.map(|then| now.duration_since(then).as_secs_f64());
        self.last = Some(now);
        match self.filter.as_mut() {
            Some(filter) => {
                let bands: Vec<f64> = bands.iter().map(|b| f64::from(*b)).collect();
                filter
                    .step(&bands, dt)
                    .ok()
                    .map(|values| values.iter().map(|v| *v as f32).collect())
            }
            None => Some(bands.to_vec()),
        }
    }
}

/// What a monitor is asked to measure.
pub struct MonitorSpec {
    pub device: Option<ObjectId>,
    pub rate_hz: f32,
    pub bands: usize,
    pub beat: bool,
    pub delay: MonitorDelay,
}

/// The lists and signals after a change: every row, as it is now.
pub struct Rows {
    pub sinks: Vec<IpcValue>,
    pub sources: Vec<IpcValue>,
    pub streams: Vec<IpcValue>,
    pub available: bool,
    /// Moves on every change, so a reader that depends on any of it reruns.
    pub revision: i64,
}

/// What one [`Session::poll`] asks of the shell.
pub struct Polled<H> {
    /// The rows when anything changed.
    pub rows: Option<Rows>,
    /// Handlers to call, in order, with their arguments.
    pub calls: Vec<(H, Vec<IpcValue>)>,
    pub errors: Vec<String>,
}

/// The audio, its handlers and its monitors.
pub struct Session<H, C> {
    pub audio: Option<Audio>,
    /// What starts the audio when first asked; the machine's server unless
    /// a host (a test) said otherwise.
    pub factory: Option<Box<dyn FnOnce() -> Audio>>,
    pub revisions: i64,
    pub listeners: Vec<(u64, H)>,
    pub monitors: HashMap<u64, Monitor<H, C>>,
    pub next_listener: u64,
}

impl<H, C> Default for Session<H, C> {
    fn default() -> Self {
        Self {
            audio: None,
            factory: None,
            revisions: 0,
            listeners: Vec::new(),
            monitors: HashMap::new(),
            next_listener: 1,
        }
    }
}

fn table(fields: BTreeMap<String, IpcValue>) -> IpcValue {
    IpcValue::Table(Arc::new(IpcTable::Map(fields)))
}

impl<H: Clone, C> Session<H, C> {
    /// The audio, started on first use.
    pub fn started(&mut self) -> &mut Audio {
        let factory = &mut self.factory;
        self.audio
            .get_or_insert_with(|| factory.take().map_or_else(Audio::connect, |start| start()))
    }

    /// Adds a change handler, starting the audio; its id, to remove it by.
    pub fn listen(&mut self, handler: H) -> Result<u64, String> {
        if self.listeners.len() >= MAX_LISTENERS {
            return Err("too many morf.audio.on_changed handlers".into());
        }
        self.started();
        let id = self.next_listener;
        self.next_listener += 1;
        self.listeners.push((id, handler));
        Ok(id)
    }

    pub fn unlisten(&mut self, id: u64) {
        self.listeners.retain(|(listener, _)| *listener != id);
    }

    /// Starts a monitor; its id.
    pub fn monitor(&mut self, spec: MonitorSpec, monitor: Monitor<H, C>) -> Result<u64, String> {
        if self.monitors.len() >= MAX_MONITORS {
            return Err("too many audio monitors running".into());
        }
        let id = self.started().monitor_delayed(
            spec.device,
            spec.rate_hz,
            spec.bands,
            spec.beat,
            spec.delay,
        );
        self.monitors.insert(id, monitor);
        Ok(id)
    }

    pub fn stop_monitor(&mut self, id: u64) {
        if self.monitors.remove(&id).is_some() {
            self.started().stop_monitor(id);
        }
    }

    /// The tempo a monitor last heard: bpm and confidence.
    pub fn tempo(&self, id: u64) -> Option<(f32, f32)> {
        self.monitors.get(&id).and_then(|monitor| monitor.tempo)
    }

    /// Takes in what the server reported. Bands for a channel are written
    /// with `write`; everything a handler hears comes back in the calls.
    /// Nothing when the audio has not started.
    pub fn poll(&mut self, mut write: impl FnMut(&C, &[f32])) -> Option<Polled<H>> {
        let audio = self.audio.as_mut()?;
        let poll = audio.poll();
        let mut calls = Vec::new();
        let mut rows = None;
        if poll.changes.any() {
            let snapshot = audio.state();
            let devices = |kind| {
                snapshot
                    .devices(kind)
                    .map(|device| device_row(device, snapshot.is_default(device.id)))
                    .collect::<Vec<_>>()
            };
            self.revisions += 1;
            rows = Some(Rows {
                sinks: devices(DeviceKind::Sink),
                sources: devices(DeviceKind::Source),
                streams: snapshot.streams().map(stream_row).collect(),
                available: snapshot.available(),
                revision: self.revisions,
            });
            let what = table(BTreeMap::from([
                ("available".into(), poll.changes.available.into()),
                ("devices".into(), poll.changes.devices.into()),
                ("streams".into(), poll.changes.streams.into()),
                ("defaults".into(), poll.changes.defaults.into()),
            ]));
            for (_, handler) in &self.listeners {
                calls.push((handler.clone(), vec![what.clone()]));
            }
        }
        for level in &poll.levels {
            let Some(out) = self
                .monitors
                .get_mut(&level.monitor)
                .and_then(|monitor| monitor.channel.as_mut())
            else {
                continue;
            };
            if let Some(values) = out.step(&level.bands) {
                write(&out.channel, &values);
            }
        }
        for level in poll.levels {
            let Some(handler) = self
                .monitors
                .get(&level.monitor)
                .and_then(|monitor| monitor.on_level.as_ref())
            else {
                continue;
            };
            let bands = if level.bands.is_empty() {
                IpcValue::Nil
            } else {
                IpcValue::Table(Arc::new(IpcTable::List(
                    level
                        .bands
                        .iter()
                        .map(|band| IpcValue::Number(f64::from(*band)))
                        .collect(),
                )))
            };
            calls.push((
                handler.clone(),
                vec![
                    IpcValue::Number(f64::from(level.left)),
                    IpcValue::Number(f64::from(level.right)),
                    bands,
                ],
            ));
        }
        for beat in poll.beats {
            if let Some(handler) = self
                .monitors
                .get(&beat.monitor)
                .and_then(|monitor| monitor.on_beat.as_ref())
            {
                calls.push((
                    handler.clone(),
                    vec![IpcValue::Number(f64::from(beat.strength))],
                ));
            }
        }
        for tempo in poll.tempos {
            let Some(monitor) = self.monitors.get_mut(&tempo.monitor) else {
                continue;
            };
            monitor.tempo = Some((tempo.bpm, tempo.confidence));
            if let Some(handler) = &monitor.on_tempo {
                calls.push((
                    handler.clone(),
                    vec![
                        IpcValue::Number(f64::from(tempo.bpm)),
                        IpcValue::Number(f64::from(tempo.confidence)),
                    ],
                ));
            }
        }
        Some(Polled {
            rows,
            calls,
            errors: poll.errors,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listeners_are_counted_and_removed() {
        let mut session: Session<u32, ()> = Session::default();
        session.factory = Some(Box::new(Audio::unavailable));
        for handler in 0..MAX_LISTENERS as u32 {
            session.listen(handler).unwrap();
        }
        assert!(session.listen(99).is_err());
        session.unlisten(1);
        assert_eq!(session.listeners.len(), MAX_LISTENERS - 1);
        assert!(session.listen(99).is_ok());
    }

    #[test]
    fn an_unstarted_session_polls_nothing() {
        let mut session: Session<u32, ()> = Session::default();
        assert!(session.poll(|_, _| {}).is_none());
    }

    #[test]
    fn a_channel_without_a_filter_takes_the_bands_as_they_are() {
        let mut channel = MonitorChannel::new((), None);
        assert_eq!(channel.step(&[0.25, 0.5]), Some(vec![0.25, 0.5]));
        assert!(channel.last.is_some());
    }
}
