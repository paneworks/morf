//! `morf.toplevels` — the compositor's windows, followed rather than polled.
//!
//! ```lua
//! local toplevels = morf.toplevels
//! toplevels.list()          -- { { identifier, title, app_id, activated, ... } }
//! toplevels.get(identifier) -- one of those, or nil
//! toplevels.revision()      -- a number that moves whenever anything does
//! toplevels.model           -- a list model keyed by identifier, for a Repeater
//! toplevels.on_changed(function(change) end)
//!                           -- change = { opened = {ids}, closed = {ids}, changed = {ids} }
//! ```
//!
//! `list`, `get` and `revision` are tracked, so a binding or an effect that
//! calls one re-runs when a window opens, closes, or changes its title,
//! app id or state. The model is reconciled by identifier, so a Repeater
//! over it keeps each row's delegate across a retitle instead of rebuilding
//! the dock. `morf.windows` stays the plain snapshot it always was.

use luna::{
    Callback, CallbackReturn, Closure, Context, Executor, StashedClosure, Table, UserData,
    Value as LuaValue, Variadic,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use morf_scene::reactive::SignalId;
use morf_scene::{ListModel, Value as SceneValue};

use crate::{
    reactive_execute::drive_executor, scene_bindings::*, serialization::scene_to_lua, state::*,
    state_tokens::*, surface_types::*, types::*,
};

/// How many `on_changed` handlers one configuration may hold.
const MAX_LISTENERS: usize = 32;

/// What `morf.toplevels` keeps between compositor updates.
pub(crate) struct ToplevelHost {
    /// The last list the compositor reported, in its order.
    pub(crate) windows: Vec<Toplevel>,
    pub(crate) model: Rc<RefCell<ListModel>>,
    pub(crate) revision: SignalId,
    pub(crate) revisions: i64,
    pub(crate) listeners: Vec<(u64, StashedClosure)>,
    pub(crate) next_listener: u64,
}

/// What moved between two window lists, by identifier.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ToplevelChange {
    pub(crate) opened: Vec<String>,
    pub(crate) closed: Vec<String>,
    pub(crate) changed: Vec<String>,
}

impl ToplevelChange {
    pub(crate) fn between(before: &[Toplevel], after: &[Toplevel]) -> Self {
        let old = before
            .iter()
            .map(|window| (window.identifier.as_str(), window))
            .collect::<std::collections::HashMap<_, _>>();
        let new = after
            .iter()
            .map(|window| window.identifier.as_str())
            .collect::<std::collections::HashSet<_>>();
        let mut change = Self::default();
        for window in after {
            match old.get(window.identifier.as_str()) {
                None => change.opened.push(window.identifier.clone()),
                Some(previous) if *previous != window => {
                    change.changed.push(window.identifier.clone());
                }
                Some(_) => {}
            }
        }
        for window in before {
            if !new.contains(window.identifier.as_str()) {
                change.closed.push(window.identifier.clone());
            }
        }
        change
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.opened.is_empty() && self.closed.is_empty() && self.changed.is_empty()
    }

    pub(crate) fn to_scene(&self) -> SceneValue {
        let list = |ids: &[String]| {
            SceneValue::List(ids.iter().cloned().map(SceneValue::String).collect())
        };
        SceneValue::Map(BTreeMap::from([
            ("opened".into(), list(&self.opened)),
            ("closed".into(), list(&self.closed)),
            ("changed".into(), list(&self.changed)),
        ]))
    }
}

/// One window as a model row and as what `list()` hands out.
pub(crate) fn toplevel_row(window: &Toplevel) -> SceneValue {
    SceneValue::Map(BTreeMap::from([
        (
            "identifier".into(),
            SceneValue::String(window.identifier.clone()),
        ),
        ("title".into(), SceneValue::String(window.title.clone())),
        ("app_id".into(), SceneValue::String(window.app_id.clone())),
        ("activated".into(), SceneValue::Bool(window.activated)),
        ("maximized".into(), SceneValue::Bool(window.maximized)),
        ("minimized".into(), SceneValue::Bool(window.minimized)),
        ("fullscreen".into(), SceneValue::Bool(window.fullscreen)),
        ("controllable".into(), SceneValue::Bool(window.controllable)),
        (
            "outputs".into(),
            SceneValue::List(
                window
                    .outputs
                    .iter()
                    .map(|name| SceneValue::String(name.clone()))
                    .collect(),
            ),
        ),
        // The first output, for the common question "which screen is it on";
        // absent while the compositor has not said.
        (
            "output".into(),
            window
                .outputs
                .first()
                .map_or(SceneValue::Nil, |name| SceneValue::String(name.clone())),
        ),
        (
            "parent".into(),
            window
                .parent
                .clone()
                .map_or(SceneValue::Nil, SceneValue::String),
        ),
    ]))
}

fn host(state: &mut ReactiveState) -> &mut ToplevelHost {
    state
        .toplevels
        .as_mut()
        .expect("morf.toplevels is installed with the runtime")
}

/// Marks the running binding, if any, as following the window list.
fn follow(state: &mut ReactiveState) {
    let revision = host(state).revision;
    if let Some(active) = &mut state.active {
        active.reads.insert(revision);
    }
}

pub(crate) fn install_toplevels_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    {
        let mut state = state.borrow_mut();
        let revision = state
            .graph
            .as_mut()
            .expect("the graph is not running at install")
            .signal("toplevels.revision", IpcValue::Integer(0));
        state.values.insert(revision, IpcValue::Integer(0));
        state.signals.push(revision);
        state.toplevels = Some(ToplevelHost {
            windows: Vec::new(),
            model: Rc::new(RefCell::new(ListModel::default())),
            revision,
            revisions: 0,
            listeners: Vec::new(),
            next_listener: 1,
        });
    }

    let toplevels = Table::new(&ctx);
    toplevels.set_field(
        ctx,
        "list",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let mut state = state.borrow_mut();
                follow(&mut state);
                let list = Table::new(&ctx);
                for (index, window) in host(&mut state).windows.iter().enumerate() {
                    list.set(
                        ctx,
                        index as i64 + 1,
                        scene_to_lua(ctx, &toplevel_row(window)).map_err(HostError)?,
                    )?;
                }
                stack.replace(ctx, list);
                Ok(CallbackReturn::Return)
            }
        }),
    );
    toplevels.set_field(
        ctx,
        "get",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let identifier: String = stack.consume(ctx)?;
                let mut state = state.borrow_mut();
                follow(&mut state);
                let value = match host(&mut state)
                    .windows
                    .iter()
                    .find(|window| window.identifier == identifier)
                {
                    Some(window) => scene_to_lua(ctx, &toplevel_row(window)).map_err(HostError)?,
                    None => LuaValue::Nil,
                };
                stack.replace(ctx, value);
                Ok(CallbackReturn::Return)
            }
        }),
    );
    toplevels.set_field(
        ctx,
        "revision",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let mut state = state.borrow_mut();
                follow(&mut state);
                let revision = host(&mut state).revisions;
                stack.replace(ctx, revision);
                Ok(CallbackReturn::Return)
            }
        }),
    );
    toplevels.set_field(
        ctx,
        "on_changed",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let callback: Closure = stack.consume(ctx)?;
                let id = {
                    let mut state = state.borrow_mut();
                    let host = host(&mut state);
                    if host.listeners.len() >= MAX_LISTENERS {
                        return Err(HostError(
                            "too many morf.toplevels.on_changed handlers".into(),
                        )
                        .into());
                    }
                    let id = host.next_listener;
                    host.next_listener += 1;
                    host.listeners.push((id, ctx.stash(callback)));
                    id
                };
                let stop = Callback::from_fn(&ctx, {
                    let state = Rc::clone(&state);
                    move |_, _, _| {
                        host(&mut state.borrow_mut())
                            .listeners
                            .retain(|(listener, _)| *listener != id);
                        Ok(CallbackReturn::Return)
                    }
                });
                let handle = Table::new(&ctx);
                handle.set_field(ctx, "stop", stop);
                stack.replace(ctx, handle);
                Ok(CallbackReturn::Return)
            }
        }),
    );
    // The model through `__index`, so every read hands out the same model
    // (the one the engine reconciles) behind a fresh handle.
    let index = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (_, key): (Table, String) = stack.consume(ctx)?;
            if key != "model" {
                stack.replace(ctx, LuaValue::Nil);
                return Ok(CallbackReturn::Return);
            }
            let mut state = state.borrow_mut();
            let model = Rc::clone(&host(&mut state).model);
            let metatable = state
                .model_metatable
                .clone()
                .ok_or_else(|| HostError("list models are not installed".into()))?;
            let userdata = UserData::new_static(&ctx, ListModelToken { model });
            userdata.set_metatable(ctx, Some(ctx.fetch(&metatable)));
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        }
    });
    let metatable = Table::new(&ctx);
    metatable.set_field(ctx, "__index", index);
    toplevels.set_metatable(ctx, Some(metatable));
    morf.set_field(ctx, "toplevels", toplevels);
}

/// Runs one `on_changed` handler with its change table.
pub(crate) fn execute_toplevel_handler(
    ctx: Context<'_>,
    closure: &StashedClosure,
    change: &SceneValue,
    limits: Limits,
) -> Result<(), String> {
    let change = scene_to_lua(ctx, change)?;
    let executor = Executor::start(ctx, ctx.fetch(closure).into(), Variadic(vec![change]));
    drive_executor(
        ctx,
        executor,
        limits,
        limits.effect_fuel,
        "toplevels handler",
    )?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}
