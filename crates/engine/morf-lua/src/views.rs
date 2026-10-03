use luna::{Context, Executor, Function, StashedClosure, UserRef, Value as LuaValue, Variadic};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use morf_scene::{
    Element, ListChange, ModelId, NodeHandle, Scene, Value as SceneValue, ViewTransition,
};

use crate::{
    reactive_bindings::*,
    reactive_execute::*,
    runtime_helpers::{begin_node_exit, cancel_node_exit, remove_scene_subtree},
    scene_bindings::*,
    serialization::*,
    state::*,
    types::*,
};

pub(crate) fn execute_delegate(
    ctx: Context<'_>,
    delegate: &StashedClosure,
    item: &SceneValue,
    index: usize,
    limits: Limits,
) -> Result<DelegateInstance, String> {
    let args = Variadic(vec![
        scene_to_lua(ctx, item)?,
        LuaValue::Integer(index as i64 + 1),
    ]);
    let executor = Executor::start(ctx, ctx.fetch(delegate).into(), args);
    drive_executor(ctx, executor, limits, limits.delegate_fuel, "delegate")?;
    let values = match executor.take_result::<Variadic<Vec<LuaValue>>>(ctx) {
        Ok(Ok(values)) => values,
        Ok(Err(error)) => return Err(error.to_string()),
        Err(error) => return Err(error.to_string()),
    };
    let Some(LuaValue::UserData(node)) = values.first().copied() else {
        return Err("view delegate must return a morf node".to_owned());
    };
    let node = node
        .downcast_static::<NodeToken>()
        .map_err(|_| "view delegate must return a morf node".to_owned())?;
    let updater = match values.get(1).copied().unwrap_or(LuaValue::Nil) {
        LuaValue::Nil => None,
        LuaValue::Function(Function::Closure(updater)) => Some(ctx.stash(updater)),
        _ => return Err("view delegate updater must be a function".to_owned()),
    };
    Ok(DelegateInstance {
        node: node.handle,
        updater,
        item: item.clone(),
        index,
    })
}

pub(crate) fn execute_delegate_updater(
    ctx: Context<'_>,
    updater: &StashedClosure,
    item: &SceneValue,
    index: usize,
    limits: Limits,
) -> Result<(), String> {
    let args = Variadic(vec![
        scene_to_lua(ctx, item)?,
        LuaValue::Integer(index as i64 + 1),
    ]);
    let executor = Executor::start(ctx, ctx.fetch(updater).into(), args);
    drive_executor(
        ctx,
        executor,
        limits,
        limits.effect_fuel,
        "delegate updater",
    )?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn execute_node_factory(
    ctx: Context<'_>,
    factory: &StashedClosure,
    limits: Limits,
) -> Result<NodeHandle, String> {
    let executor = Executor::start(ctx, ctx.fetch(factory).into(), ());
    drive_executor(ctx, executor, limits, limits.effect_fuel, "Loader source")?;
    match executor.take_result::<UserRef<NodeToken>>(ctx) {
        Ok(Ok(node)) => Ok(node.handle),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn position_view_child(
    scene: &mut Scene,
    node: NodeHandle,
    index: usize,
    view: &morf_scene::VirtualList,
    offset: f64,
    column_extent: f64,
) -> Result<(), String> {
    scene
        .assign(node, "x", (index % view.columns()) as f64 * column_extent)
        .map_err(|error| error.to_string())?;
    scene
        .assign(node, "y", view.item_start(index) - offset)
        .map_err(|error| error.to_string())
}

/// A row field read as a number (a row's extent), if the row has it.
pub(crate) fn row_number(item: &SceneValue, field: &str) -> Option<f64> {
    match item {
        SceneValue::Map(map) => match map.get(field)? {
            SceneValue::Number(n) => Some(*n),
            _ => None,
        },
        _ => None,
    }
}

/// A row field read as text (a row's kind), if the row has it.
pub(crate) fn row_kind<'a>(item: &'a SceneValue, field: Option<&str>) -> Option<&'a str> {
    match (item, field) {
        (SceneValue::Map(map), Some(field)) => match map.get(field)? {
            SceneValue::String(kind) => Some(kind),
            _ => None,
        },
        _ => None,
    }
}

/// Each row's extent, by its size field, for a list whose rows differ.
pub(crate) fn row_extents(model: &morf_scene::ListModel, field: &str, fallback: f64) -> Vec<f64> {
    (0..model.len())
        .map(|index| {
            model
                .get(index)
                .and_then(|(_, item)| row_number(item, field))
                .unwrap_or(fallback)
        })
        .collect()
}

pub(crate) fn reconcile_lua_view(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
    parent: NodeHandle,
    offset: f64,
    view: &mut LuaVirtualView,
) -> Result<Vec<ViewTransition>, String> {
    if !offset.is_finite() || offset < 0.0 {
        return Err("ListView offset must be finite and non-negative".to_owned());
    }
    view.view.set_offset(offset);
    let changes = view.model.borrow_mut().take_changes();
    let updated = changes
        .iter()
        .filter_map(|change| match change {
            ListChange::Updated { id, .. } => Some(*id),
            _ => None,
        })
        .collect::<HashSet<_>>();
    // An updated item whose live delegate came with an updater is patched in
    // place and keeps its node; one without is rebuilt like a removal and an
    // insertion. The difference is what a spring on that node, or a texture
    // it published, survives.
    let patched = updated
        .iter()
        .copied()
        .filter(|id| {
            view.active
                .get(id)
                .is_some_and(|instance| instance.updater.is_some())
        })
        .collect::<HashSet<_>>();
    let invalidated = changes
        .iter()
        .filter_map(|change| match change {
            ListChange::Removed { id, .. } => Some(*id),
            ListChange::Updated { id, .. } if !patched.contains(id) => Some(*id),
            _ => None,
        })
        .collect::<HashSet<_>>();
    // Rows the model let go of, as opposed to rows rebuilt: these play their
    // delegate's exit, if it has one, before they are removed.
    let removed_rows = changes
        .iter()
        .filter_map(|change| match change {
            ListChange::Removed { id, .. } => Some(*id),
            _ => None,
        })
        .collect::<HashSet<_>>();
    // Whatever finished leaving since, or was removed some other way.
    {
        let state = state.borrow();
        view.exiting
            .retain(|instance| state.scene.is_exiting(instance.node));
    }
    for id in &invalidated {
        if let Some(instance) = view.reusable.remove(id) {
            remove_scene_subtree(&mut state.borrow_mut(), instance.node);
        }
    }
    view.reuse_order.retain(|id| !invalidated.contains(id));
    let model = view.model.borrow();
    // The viewport is the node's height as it is now: a view in a panel
    // that grows shows more rows.
    if view.positioned
        && let Ok(height) = state.borrow().scene.number(parent, "height")
        && height > 0.0
    {
        view.view.set_viewport(height);
        // The pool keeps as many rows as two screens of them: a view made
        // before its size was known (a bound height) learns it here.
        let shown = view.view.visible_range(view.model.borrow().len()).len();
        view.reuse_limit = view.reuse_limit.max(shown.max(1) * 2);
    }
    if let Some(field) = view.size_field.clone() {
        let extents = row_extents(&model, &field, view.view.item_extent());
        view.view.set_extents(&extents);
    }
    let transitions = view.view.sync(&model, &changes);
    let visible = view
        .view
        .visible_range(model.len())
        .filter_map(|index| {
            model
                .get(index)
                .map(|(id, value)| (id, index, value.clone()))
        })
        .collect::<Vec<_>>();
    drop(model);
    let visible_ids = visible.iter().map(|(id, _, _)| *id).collect::<HashSet<_>>();
    // Rows gone out of sight go to the pool first, so the rows coming into
    // sight in the same turn are rebound from it rather than built.
    let removed = view
        .active
        .iter()
        .filter(|(id, _)| {
            !visible_ids.contains(id) || (updated.contains(id) && !patched.contains(id))
        })
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    for id in removed {
        let instance = view.active.remove(&id).expect("removed delegate is active");
        if removed_rows.contains(&id) && begin_node_exit(&mut state.borrow_mut(), instance.node) {
            // Stays where it is, drawn, until its exit ends.
            view.exiting.push(instance);
            continue;
        }
        // A row the model changed, whose delegate cannot patch itself, is
        // built again: its old delegate is no use to any row.
        if invalidated.contains(&id) || (updated.contains(&id) && !patched.contains(&id)) {
            remove_scene_subtree(&mut state.borrow_mut(), instance.node);
            continue;
        }
        let pool_root = match view.pool_root {
            Some(node) => node,
            None => {
                let pool = create_node(state, Element::Item);
                state
                    .borrow_mut()
                    .scene
                    .assign(pool, "visible", false)
                    .map_err(|error| error.to_string())?;
                state
                    .borrow_mut()
                    .scene
                    .reparent(pool, Some(parent))
                    .map_err(|error| error.to_string())?;
                view.pool_root = Some(pool);
                pool
            }
        };
        state
            .borrow_mut()
            .scene
            .reparent(instance.node, Some(pool_root))
            .map_err(|error| error.to_string())?;
        view.reusable.insert(id, instance);
        view.reuse_order.push_back(id);
    }
    while view.reusable.len() > view.reuse_limit {
        let Some(id) = view.reuse_order.pop_front() else {
            break;
        };
        if let Some(instance) = view.reusable.remove(&id) {
            remove_scene_subtree(&mut state.borrow_mut(), instance.node);
        }
    }
    let mut prepared: Vec<(ModelId, usize, DelegateInstance)> = Vec::new();
    for (id, index, item) in &visible {
        if patched.contains(id) {
            let instance = view.active.get(id).expect("patched delegates are active");
            let updater = instance
                .updater
                .as_ref()
                .expect("patched delegates have updaters");
            let update = execute_delegate_updater(ctx, updater, item, *index, limits)
                .and_then(|()| flush_reactive(state, ctx, limits));
            if let Err(error) = update {
                for (_, _, prepared) in prepared {
                    remove_scene_subtree(&mut state.borrow_mut(), prepared.node);
                }
                return Err(error);
            }
            if let Some(instance) = view.active.get_mut(id) {
                instance.item = item.clone();
                instance.index = *index;
            }
            continue;
        }
        // A row that kept its delegate but not its place -- moved, or shifted
        // by rows coming or going above it -- is told its new index, so what
        // it reads of its place (`s.index()`) is not left behind.
        if !updated.contains(id)
            && let Some(instance) = view.active.get(id)
            && instance.index != *index
            && instance.updater.is_some()
        {
            let updater = instance.updater.as_ref().expect("checked above");
            let update = execute_delegate_updater(ctx, updater, item, *index, limits)
                .and_then(|()| flush_reactive(state, ctx, limits));
            if let Err(error) = update {
                for (_, _, prepared) in prepared {
                    remove_scene_subtree(&mut state.borrow_mut(), prepared.node);
                }
                return Err(error);
            }
            if let Some(instance) = view.active.get_mut(id) {
                instance.index = *index;
            }
            continue;
        }
        if !view.active.contains_key(id) || updated.contains(id) {
            if let Some(instance) = view.reusable.remove(id) {
                view.reuse_order.retain(|candidate| candidate != id);
                prepared.push((*id, *index, instance));
                continue;
            }
            // The same row put back while its delegate is still leaving: that
            // delegate is taken back, and animates back from where it got to.
            if !removed_rows.contains(id)
                && let Some(position) = view
                    .exiting
                    .iter()
                    .position(|instance| instance.item == *item)
            {
                let instance = view.exiting.swap_remove(position);
                if cancel_node_exit(&mut state.borrow_mut(), instance.node) {
                    prepared.push((*id, *index, instance));
                    continue;
                }
            }
            // Rebinding a pooled delegate to this row, rather than building
            // one: only a delegate of the row's own kind.
            let kind = row_kind(item, view.kind_field.as_deref());
            let reusable_id = view.reuse_order.iter().copied().find(|candidate| {
                view.reusable.get(candidate).is_some_and(|instance| {
                    instance.updater.is_some()
                        && row_kind(&instance.item, view.kind_field.as_deref()) == kind
                })
            });
            if let Some(reusable_id) = reusable_id {
                view.reuse_order
                    .retain(|candidate| *candidate != reusable_id);
                let instance = view
                    .reusable
                    .remove(&reusable_id)
                    .expect("reuse order contains a live delegate");
                let mut instance = instance;
                instance.item = item.clone();
                let update = execute_delegate_updater(
                    ctx,
                    instance.updater.as_ref().expect("updater was checked"),
                    item,
                    *index,
                    limits,
                )
                .and_then(|()| flush_reactive(state, ctx, limits));
                if let Err(error) = update {
                    remove_scene_subtree(&mut state.borrow_mut(), instance.node);
                    for (_, _, prepared) in prepared {
                        remove_scene_subtree(&mut state.borrow_mut(), prepared.node);
                    }
                    return Err(error);
                }
                prepared.push((*id, *index, instance));
                continue;
            }
            match execute_delegate(ctx, &view.delegate, item, *index, limits) {
                Ok(instance) => prepared.push((*id, *index, instance)),
                Err(error) => {
                    for (_, _, prepared) in prepared {
                        remove_scene_subtree(&mut state.borrow_mut(), prepared.node);
                    }
                    return Err(error);
                }
            }
        }
    }
    for (id, index, mut instance) in prepared {
        instance.index = index;
        if view.positioned {
            position_view_child(
                &mut state.borrow_mut().scene,
                instance.node,
                index,
                &view.view,
                offset,
                view.column_extent,
            )?;
        }
        state
            .borrow_mut()
            .scene
            .reparent(instance.node, Some(parent))
            .map_err(|error| error.to_string())?;
        view.active.insert(id, instance);
    }
    if view.positioned {
        for (id, index, _) in &visible {
            if let Some(instance) = view.active.get(id) {
                position_view_child(
                    &mut state.borrow_mut().scene,
                    instance.node,
                    *index,
                    &view.view,
                    offset,
                    view.column_extent,
                )?;
            }
        }
    } else {
        // Nothing positions these delegates but their order, so the order
        // is the model's: a Row of them reads as the list does, and a moved
        // item moves on screen.
        let order = visible
            .iter()
            .filter_map(|(id, _, _)| view.active.get(id).map(|instance| instance.node))
            .collect::<Vec<_>>();
        state
            .borrow_mut()
            .scene
            .reorder_children(parent, &order)
            .map_err(|error| error.to_string())?;
    }
    Ok(transitions)
}
