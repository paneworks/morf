//! Keeping `morf.prefers` and theme sources current between frames.

use std::time::Duration;

use morf_scene::reactive::SignalId;

use crate::{
    api_prefers::*, api_theme::read_tokens, reactive_bindings::*, state::*, surface_types::*,
    types::*,
};

/// Writes one signal from the host side; the caller flushes.
fn write_signal(state: &mut ReactiveState, id: SignalId, value: IpcValue) -> Result<(), String> {
    if state.reactive.values.get(&id) == Some(&value) {
        return Ok(());
    }
    state
        .reactive
        .graph
        .as_mut()
        .ok_or_else(|| "reactive graph is already running".to_owned())?
        .write(id, value.clone())
        .map_err(|error| error.to_string())?;
    state.reactive.values.insert(id, value);
    Ok(())
}

impl Runtime {
    /// Sets one of `morf.prefers`' fields as if the desktop had.
    ///
    /// What the settings portal does when a setting changes, offered here so
    /// a host without a portal — a lock screen, a test — can say the same.
    pub fn set_preference(&mut self, name: &str, value: IpcValue) -> Result<(), String> {
        let Some(&name) = PREFERENCES.iter().find(|each| **each == name) else {
            return Err(format!("`{name}` is not a preference"));
        };
        if let Some(prefers) = &mut self.reactive.borrow_mut().prefers {
            prefers.overridden.insert(name);
        }
        self.write_preference(name, value)?;
        self.lua
            .enter(|ctx| flush_reactive(&self.reactive, ctx, self.limits))
    }

    fn write_preference(&mut self, name: &str, value: IpcValue) -> Result<(), String> {
        let mut state = self.reactive.borrow_mut();
        let Some(prefers) = &state.prefers else {
            return Ok(());
        };
        let id = match name {
            "color_scheme" => prefers.color_scheme,
            "contrast" => prefers.contrast,
            "reduced_motion" => prefers.reduced_motion,
            "accent_color" => prefers.accent_color,
            _ => prefers.scale,
        };
        if name == "reduced_motion" {
            let reduced = matches!(value, IpcValue::Boolean(true));
            state
                .scene
                .set_motion_scale(if reduced { 0.0 } else { 1.0 });
        }
        write_signal(&mut state, id, value)
    }

    /// The driven output's scale, for `morf.prefers.scale`.
    pub(crate) fn set_preferred_scale(&mut self, scale: i32) {
        if let Err(message) = self.set_preference("scale", IpcValue::Integer(i64::from(scale))) {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("preferences: {message}"));
        }
    }

    /// Delivers portal changes and rewritten theme files. True when a
    /// signal moved.
    pub(crate) fn poll_appearance(&mut self) -> bool {
        let mut changes: Vec<(&'static str, IpcValue)> = Vec::new();
        let mut rewritten: Vec<usize> = Vec::new();
        {
            let mut state = self.reactive.borrow_mut();
            if let Some(Prefers {
                portal: Some(portal),
                overridden,
                ..
            }) = &mut state.prefers
            {
                changes = portal.poll(overridden);
            }
            for (index, source) in state.theme_sources.iter().enumerate() {
                let Some(watcher) = &source.watcher else {
                    continue;
                };
                let mut touched = false;
                while watcher.next_event(Duration::ZERO).is_some() {
                    touched = true;
                }
                if touched {
                    rewritten.push(index);
                }
            }
        }
        let mut moved = false;
        for (name, value) in changes {
            match self.write_preference(name, value) {
                Ok(()) => moved = true,
                Err(message) => self
                    .reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("preferences: {message}")),
            }
        }
        for index in rewritten {
            let (path, fields) = {
                let state = self.reactive.borrow();
                let source = &state.theme_sources[index];
                (source.path.clone(), source.fields.clone())
            };
            if !path.exists() {
                continue;
            }
            let tokens = match read_tokens(&path) {
                Ok(tokens) => tokens,
                Err(message) => {
                    self.reactive
                        .borrow_mut()
                        .log(LogLevel::Warn, format!("theme source: {message}"));
                    continue;
                }
            };
            let mut state = self.reactive.borrow_mut();
            for (key, value) in tokens {
                if let Some(&id) = fields.get(&key) {
                    match write_signal(&mut state, id, value) {
                        Ok(()) => moved = true,
                        Err(message) => {
                            state.log(LogLevel::Warn, format!("theme source: {message}"))
                        }
                    }
                }
            }
        }
        if moved
            && let Err(message) = self
                .lua
                .enter(|ctx| flush_reactive(&self.reactive, ctx, self.limits))
        {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("appearance: {message}"));
        }
        moved
    }
}
