//! A list model's reads are dependencies.
//!
//! A binding that reads `model:len()` or `model:get(i)` computes from the
//! model, and has to run again when the model changes. The model is not a
//! signal -- it is a journal of rows that a `Repeater` follows -- so each
//! model a binding has read gets one: a revision, bumped on every change.
//! The signal is made the first time a binding reads the model, so a model
//! nothing computes from costs the graph nothing.

use luna::Context;
use morf_scene::ListModel;
use std::cell::RefCell;
use std::rc::Rc;

use crate::{
    reactive_bindings::flush_reactive, scene_bindings::HostError, serialization::scene_to_lua,
    state::ReactiveState, types::*,
};

/// Notes that the running binding read `model`.
pub(crate) fn track_model_read(state: &mut ReactiveState, model: &Rc<RefCell<ListModel>>) {
    let signal = state.model_revisions.signal(model);
    if let Some(active) = &mut state.active {
        match signal {
            Some(signal) => {
                active.reads.insert(signal);
            }
            None => {
                if !active
                    .model_reads
                    .iter()
                    .any(|read| Rc::ptr_eq(read, model))
                {
                    active.model_reads.push(Rc::clone(model));
                }
            }
        }
    }
}

/// The revision signal of each model the finished binding read, made now
/// if this is the first binding to read it, for the binding to depend on.
pub(crate) fn model_read_signals(
    state: &Rc<RefCell<ReactiveState>>,
    models: Vec<Rc<RefCell<ListModel>>>,
) -> Result<Vec<morf_scene::reactive::SignalId>, String> {
    let mut signals = Vec::with_capacity(models.len());
    let mut state = state.borrow_mut();
    let state = &mut *state;
    for model in models {
        signals.push(
            state
                .model_revisions
                .signal_or_make(&mut state.reactive, &model)?,
        );
    }
    Ok(signals)
}

/// Bumps a changed model's revision, if any binding ever read it. Returns
/// whether it did: the graph then owes a flush.
pub(crate) fn bump_model_revision(
    state: &mut ReactiveState,
    model: &Rc<RefCell<ListModel>>,
) -> Result<bool, String> {
    let Some((signal, value)) = state.model_revisions.bump(model) else {
        return Ok(false);
    };
    if let Some(active) = &mut state.active {
        active.writes.push((signal, value.clone()));
    } else {
        state
            .reactive
            .graph
            .as_mut()
            .ok_or_else(|| "reactive graph is already running".to_owned())?
            .write(signal, value.clone())
            .map_err(|error| error.to_string())?;
        state.flush_pending = true;
    }
    state.reactive.values.insert(signal, value);
    Ok(true)
}

/// A model changed from Lua: bump it, and flush now unless a handler or a
/// binding is running, which flush on their own -- the same rule a signal
/// write follows.
pub(crate) fn model_changed(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
    model: &Rc<RefCell<ListModel>>,
) -> Result<(), String> {
    let flush = {
        let mut state = state.borrow_mut();
        let bumped = bump_model_revision(&mut state, model)?;
        bumped && state.active.is_none() && state.handler_depth == 0
    };
    if flush {
        state.borrow_mut().flush_pending = false;
        flush_reactive(state, ctx, limits)?;
    }
    Ok(())
}

/// The row at a zero-based index, as Lua sees it; nil past the end.
pub(crate) fn model_row<'gc>(
    ctx: Context<'gc>,
    model: &Rc<RefCell<ListModel>>,
    index: usize,
) -> Result<luna::Value<'gc>, HostError> {
    let model = model.borrow();
    Ok(model
        .get(index)
        .map(|(_, value)| scene_to_lua(ctx, value))
        .transpose()
        .map_err(HostError)?
        .unwrap_or(luna::Value::Nil))
}

/// Replaces a model's rows, matched by `key` or by value, and bumps its
/// revision when that changed anything.
pub(crate) fn replace_model_rows(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
    model: &Rc<RefCell<ListModel>>,
    rows: Vec<morf_scene::Value>,
    key: Option<&str>,
) -> Result<(), String> {
    let unchanged = {
        let model = model.borrow();
        model.len() == rows.len()
            && rows
                .iter()
                .enumerate()
                .all(|(index, row)| model.get(index).is_some_and(|(_, value)| value == row))
    };
    model.borrow_mut().reconcile(rows, key);
    if unchanged {
        return Ok(());
    }
    model_changed(state, ctx, limits, model)
}

impl ReactiveState {
    /// Forgets the revision signals of models nothing holds any more.
    pub(crate) fn collect_dead_models(&mut self) {
        self.model_revisions.collect_dead(&mut self.reactive);
    }
}
