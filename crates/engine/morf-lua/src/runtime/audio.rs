//! Keeping `morf.audio` current between frames.

use std::rc::Rc;

use morf_audio::{Audio, Backend};
use morf_scene::Value as SceneValue;

use crate::{api_audio::*, reactive_bindings::flush_reactive, surface_types::*, types::*};
use morf_runtime::Handler;

impl Runtime {
    /// Gives `morf.audio` a backend of the host's choosing — a fake one in a
    /// test — in place of the machine's sound server. Takes effect if the
    /// configuration has not touched `morf.audio` yet.
    pub fn set_audio_backend(&mut self, backend: impl Backend) {
        let mut state = self.reactive.borrow_mut();
        if let Some(host) = &mut state.audio
            && host.session.audio.is_none()
        {
            host.session.factory = Some(Box::new(move || Audio::with_backend(backend)));
        }
    }

    /// Takes in what the sound server reported: rows into the list models,
    /// the reactive signals moved, then `on_changed` handlers and meters run.
    /// True when visible scene content changed. Signals and callbacks still
    /// advance when hidden, without making an idle output render again.
    pub(crate) fn poll_audio(&mut self) -> bool {
        let (revision_before, hidden_before) = {
            let state = self.reactive.borrow();
            (
                state.revisions.scene_revision,
                state.revisions.hidden_revisions,
            )
        };
        let mut moved = false;
        let handlers: Vec<(Handler, Vec<SceneValue>)>;
        {
            let mut guard = self.reactive.borrow_mut();
            let state = &mut *guard;
            let Some(host) = state.audio.as_mut() else {
                return false;
            };
            // A channel takes the bands in Rust: filtered, written, drawn.
            let Some(polled) = host.session.poll(|channel, values| channel.set(values)) else {
                return false;
            };
            handlers = polled
                .calls
                .into_iter()
                .map(|(handler, args)| (handler, args.iter().map(scene).collect()))
                .collect();
            if let Some(rows) = polled.rows {
                let mut changed_models = Vec::new();
                for (model, rows) in [
                    (&host.sinks, rows.sinks),
                    (&host.sources, rows.sources),
                    (&host.streams, rows.streams),
                ] {
                    let rows = rows.iter().map(scene).collect();
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
                let writes = [
                    (host.available, IpcValue::Boolean(rows.available)),
                    (host.revision, IpcValue::Integer(rows.revision)),
                ];
                for (id, value) in writes {
                    if state.reactive.values.get(&id) == Some(&value) {
                        continue;
                    }
                    if let Some(graph) = state.reactive.graph.as_mut()
                        && graph.write(id, value.clone()).is_ok()
                    {
                        state.reactive.values.insert(id, value);
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
            for error in polled.errors {
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
        state.revisions.scene_revision.wrapping_sub(revision_before)
            > state.revisions.hidden_revisions.wrapping_sub(hidden_before)
    }
}
