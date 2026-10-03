//! Keeping `morf.audio` current between frames.

use std::collections::BTreeMap;
use std::rc::Rc;

use morf_audio::{Audio, Backend, DeviceKind};
use morf_scene::Value as SceneValue;

use crate::{api_audio::*, reactive_bindings::flush_reactive, surface_types::*, types::*};

impl Runtime {
    /// Gives `morf.audio` a backend of the host's choosing — a fake one in a
    /// test — in place of the machine's sound server. Takes effect if the
    /// configuration has not touched `morf.audio` yet.
    pub fn set_audio_backend(&mut self, backend: impl Backend) {
        let mut state = self.reactive.borrow_mut();
        if let Some(host) = &mut state.audio
            && host.audio.is_none()
        {
            host.factory = Some(Box::new(move || Audio::with_backend(backend)));
        }
    }

    /// Takes in what the sound server reported: rows into the list models,
    /// the reactive signals moved, then `on_changed` handlers and meters run.
    /// True when visible scene content changed. Signals and callbacks still
    /// advance when hidden, without making an idle output render again.
    pub(crate) fn poll_audio(&mut self) -> bool {
        let (revision_before, hidden_before) = {
            let state = self.reactive.borrow();
            (state.scene_revision, state.hidden_revisions)
        };
        let mut handlers: Vec<(luna::StashedClosure, Vec<SceneValue>)> = Vec::new();
        let mut moved = false;
        {
            let mut guard = self.reactive.borrow_mut();
            let state = &mut *guard;
            let Some(host) = state.audio.as_mut() else {
                return false;
            };
            let Some(audio) = host.audio.as_mut() else {
                return false;
            };
            let poll = audio.poll();
            let errors = poll.errors;
            if poll.changes.any() {
                let snapshot = audio.state();
                let rows = |kind| {
                    snapshot
                        .devices(kind)
                        .map(|device| device_row(device, snapshot.is_default(device.id)))
                        .collect::<Vec<_>>()
                };
                let sinks = rows(DeviceKind::Sink);
                let sources = rows(DeviceKind::Source);
                let streams = snapshot.streams().map(stream_row).collect::<Vec<_>>();
                let available = snapshot.available();
                let mut changed_models = Vec::new();
                for (model, rows) in [
                    (&host.sinks, sinks),
                    (&host.sources, sources),
                    (&host.streams, streams),
                ] {
                    model.borrow_mut().reconcile(rows, Some("id"));
                    changed_models.push(Rc::clone(model));
                    // A list nothing draws would keep its change journal
                    // forever; one a view follows is drained by the view.
                    let followed = state
                        .views
                        .values()
                        .any(|view| Rc::ptr_eq(&view.model, model));
                    if !followed {
                        model.borrow_mut().take_changes();
                    }
                }
                host.revisions += 1;
                let writes = [
                    (host.available, IpcValue::Boolean(available)),
                    (host.revision, IpcValue::Integer(host.revisions)),
                ];
                let what = SceneValue::Map(BTreeMap::from([
                    ("available".into(), SceneValue::Bool(poll.changes.available)),
                    ("devices".into(), SceneValue::Bool(poll.changes.devices)),
                    ("streams".into(), SceneValue::Bool(poll.changes.streams)),
                    ("defaults".into(), SceneValue::Bool(poll.changes.defaults)),
                ]));
                for (_, callback) in &host.listeners {
                    handlers.push((callback.clone(), vec![what.clone()]));
                }
                for (id, value) in writes {
                    if state.values.get(&id) == Some(&value) {
                        continue;
                    }
                    if let Some(graph) = state.graph.as_mut()
                        && graph.write(id, value.clone()).is_ok()
                    {
                        state.values.insert(id, value);
                        moved = true;
                    }
                }
                // A binding that counts the devices follows them.
                for model in changed_models {
                    if crate::model_revisions::bump_model_revision(state, &model).unwrap_or(false) {
                        moved = true;
                    }
                }
            }
            let host = state.audio.as_mut().expect("checked above");
            for level in &poll.levels {
                // A channel takes the bands in Rust: filtered, written, drawn.
                let Some(out) = host.monitors.get_mut(&level.monitor).and_then(|m| m.channel.as_mut()) else {
                    continue;
                };
                let now = std::time::Instant::now();
                let dt = out.last.map(|then| now.duration_since(then).as_secs_f64());
                out.last = Some(now);
                let bands: Vec<f64> = level.bands.iter().map(|b| f64::from(*b)).collect();
                let values: Vec<f32> = match out.filter.as_mut() {
                    Some(filter) => match filter.step(&bands, dt) {
                        Ok(values) => values.iter().map(|v| *v as f32).collect(),
                        Err(_) => continue,
                    },
                    None => level.bands.clone(),
                };
                out.channel.set(&values);
            }
            for level in poll.levels {
                let Some(callback) = host
                    .monitors
                    .get(&level.monitor)
                    .and_then(|handlers| handlers.on_level.as_ref())
                else {
                    continue;
                };
                let bands = if level.bands.is_empty() {
                    SceneValue::Nil
                } else {
                    SceneValue::List(
                        level
                            .bands
                            .iter()
                            .map(|band| SceneValue::Number(f64::from(*band)))
                            .collect(),
                    )
                };
                handlers.push((
                    callback.clone(),
                    vec![
                        SceneValue::Number(f64::from(level.left)),
                        SceneValue::Number(f64::from(level.right)),
                        bands,
                    ],
                ));
            }
            for beat in poll.beats {
                if let Some(callback) = host
                    .monitors
                    .get(&beat.monitor)
                    .and_then(|monitor| monitor.on_beat.as_ref())
                {
                    handlers.push((
                        callback.clone(),
                        vec![SceneValue::Number(f64::from(beat.strength))],
                    ));
                }
            }
            for tempo in poll.tempos {
                let Some(monitor) = host.monitors.get_mut(&tempo.monitor) else {
                    continue;
                };
                monitor.tempo = Some((tempo.bpm, tempo.confidence));
                if let Some(callback) = &monitor.on_tempo {
                    handlers.push((
                        callback.clone(),
                        vec![
                            SceneValue::Number(f64::from(tempo.bpm)),
                            SceneValue::Number(f64::from(tempo.confidence)),
                        ],
                    ));
                }
            }
            for error in errors {
                state.log(LogLevel::Warn, format!("audio: {error}"));
            }
        }
        if moved
            && let Err(message) = self
                .lua
                .enter(|ctx| flush_reactive(&self.reactive, ctx, self.limits))
        {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("audio: {message}"));
        }
        for (callback, args) in handlers {
            if let Err(message) =
                self.run_handler(|ctx, limits| execute_audio_handler(ctx, &callback, &args, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("audio handler: {message}"));
            }
        }
        let state = self.reactive.borrow();
        state.scene_revision.wrapping_sub(revision_before)
            > state.hidden_revisions.wrapping_sub(hidden_before)
    }
}
