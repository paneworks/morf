//! A view catching up with its model: the changes since it last looked
//! taken, delegates of rows gone out of sight pooled, rows coming into
//! sight rebound from the pool or built, moved rows told their index, and
//! every delegate put where it goes.

use std::collections::HashSet;

use morf_scene::{ListChange, ModelId, NodeHandle, ViewTransition};

use super::{Delegate, ViewHost, VirtualView, position_view_child, row_extents, row_kind};

/// Removes what was prepared before a delegate failed, and hands the error on.
fn discard(
    host: &mut impl ViewHost,
    prepared: Vec<(ModelId, usize, Delegate)>,
    error: String,
) -> String {
    for (_, _, prepared) in prepared {
        host.remove(prepared.node);
    }
    error
}

impl VirtualView {
    /// Brings the view filling `parent` up to its model, scrolled `offset`
    /// along: the transitions its window went through.
    pub fn reconcile(
        &mut self,
        host: &mut impl ViewHost,
        parent: NodeHandle,
        offset: f64,
    ) -> Result<Vec<ViewTransition>, String> {
        if !offset.is_finite() || offset < 0.0 {
            return Err("ListView offset must be finite and non-negative".to_owned());
        }
        let view = self;
        view.view.set_offset(offset);
        let changes = view.model.borrow_mut().take_changes();
        let updated = changes
            .iter()
            .filter_map(|change| match change {
                ListChange::Updated { id, .. } => Some(*id),
                _ => None,
            })
            .collect::<HashSet<_>>();
        // An updated item whose live delegate came with an updater is patched
        // in place and keeps its node; one without is rebuilt like a removal
        // and an insertion. The difference is what a spring on that node, or
        // a texture it published, survives.
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
        // Rows the model let go of, as opposed to rows rebuilt: these play
        // their delegate's exit, if it has one, before they are removed.
        let removed_rows = changes
            .iter()
            .filter_map(|change| match change {
                ListChange::Removed { id, .. } => Some(*id),
                _ => None,
            })
            .collect::<HashSet<_>>();
        // Whatever finished leaving since, or was removed some other way.
        let exiting = std::mem::take(&mut view.exiting);
        view.exiting = host.with_scene(|scene| {
            exiting
                .into_iter()
                .filter(|instance| scene.is_exiting(instance.node))
                .collect()
        });
        for id in &invalidated {
            if let Some(instance) = view.reusable.remove(id) {
                host.remove(instance.node);
            }
        }
        view.reuse_order.retain(|id| !invalidated.contains(id));
        let model = view.model.borrow();
        // The viewport is the node's height as it is now: a view in a panel
        // that grows shows more rows.
        if view.positioned
            && let Ok(height) = host.with_scene(|scene| scene.number(parent, "height"))
            && height > 0.0
        {
            view.view.set_viewport(height);
            // The pool keeps as many rows as two screens of them: a view made
            // before its size was known (a bound height) learns it here.
            let shown = view.view.visible_range(model.len()).len();
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
            if removed_rows.contains(&id) && host.begin_exit(instance.node) {
                // Stays where it is, drawn, until its exit ends.
                view.exiting.push(instance);
                continue;
            }
            // A row the model changed, whose delegate cannot patch itself, is
            // built again: its old delegate is no use to any row.
            if invalidated.contains(&id) || (updated.contains(&id) && !patched.contains(&id)) {
                host.remove(instance.node);
                continue;
            }
            let pool_root = match view.pool_root {
                Some(node) => node,
                None => {
                    let pool = host.create();
                    host.with_scene(|scene| {
                        scene.assign(pool, "visible", false)?;
                        scene.reparent(pool, Some(parent))
                    })
                    .map_err(|error| error.to_string())?;
                    view.pool_root = Some(pool);
                    pool
                }
            };
            host.with_scene(|scene| scene.reparent(instance.node, Some(pool_root)))
                .map_err(|error| error.to_string())?;
            view.reusable.insert(id, instance);
            view.reuse_order.push_back(id);
        }
        while view.reusable.len() > view.reuse_limit {
            let Some(id) = view.reuse_order.pop_front() else {
                break;
            };
            if let Some(instance) = view.reusable.remove(&id) {
                host.remove(instance.node);
            }
        }
        let mut prepared: Vec<(ModelId, usize, Delegate)> = Vec::new();
        for (id, index, item) in &visible {
            if patched.contains(id) {
                let instance = view.active.get(id).expect("patched delegates are active");
                let updater = instance
                    .updater
                    .as_ref()
                    .expect("patched delegates have updaters");
                if let Err(error) = host.update(updater, item, *index) {
                    return Err(discard(host, prepared, error));
                }
                if let Some(instance) = view.active.get_mut(id) {
                    instance.item = item.clone();
                    instance.index = *index;
                }
                continue;
            }
            // A row that kept its delegate but not its place -- moved, or
            // shifted by rows coming or going above it -- is told its new
            // index, so what it reads of its place (`s.index()`) is not left
            // behind.
            if !updated.contains(id)
                && let Some(instance) = view.active.get(id)
                && instance.index != *index
                && instance.updater.is_some()
            {
                let updater = instance.updater.as_ref().expect("checked above");
                if let Err(error) = host.update(updater, item, *index) {
                    return Err(discard(host, prepared, error));
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
                // The same row put back while its delegate is still leaving:
                // that delegate is taken back, and animates back from where
                // it got to.
                if !removed_rows.contains(id)
                    && let Some(position) = view
                        .exiting
                        .iter()
                        .position(|instance| instance.item == *item)
                {
                    let instance = view.exiting.swap_remove(position);
                    if host.cancel_exit(instance.node) {
                        prepared.push((*id, *index, instance));
                        continue;
                    }
                }
                // Rebinding a pooled delegate to this row, rather than
                // building one: only a delegate of the row's own kind.
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
                    let mut instance = view
                        .reusable
                        .remove(&reusable_id)
                        .expect("reuse order contains a live delegate");
                    instance.item = item.clone();
                    let updater = instance.updater.as_ref().expect("updater was checked");
                    if let Err(error) = host.update(updater, item, *index) {
                        host.remove(instance.node);
                        return Err(discard(host, prepared, error));
                    }
                    prepared.push((*id, *index, instance));
                    continue;
                }
                match host.build(&view.delegate, item, *index) {
                    Ok(instance) => prepared.push((*id, *index, instance)),
                    Err(error) => return Err(discard(host, prepared, error)),
                }
            }
        }
        for (id, index, mut instance) in prepared {
            instance.index = index;
            if view.positioned {
                host.with_scene(|scene| {
                    position_view_child(
                        scene,
                        instance.node,
                        index,
                        &view.view,
                        offset,
                        view.column_extent,
                    )
                })?;
            }
            host.with_scene(|scene| scene.reparent(instance.node, Some(parent)))
                .map_err(|error| error.to_string())?;
            view.active.insert(id, instance);
        }
        if view.positioned {
            for (id, index, _) in &visible {
                if let Some(instance) = view.active.get(id) {
                    host.with_scene(|scene| {
                        position_view_child(
                            scene,
                            instance.node,
                            *index,
                            &view.view,
                            offset,
                            view.column_extent,
                        )
                    })?;
                }
            }
        } else {
            // Nothing positions these delegates but their order, so the order
            // is the model's: a Row of them reads as the list does, and a
            // moved item moves on screen.
            let order = visible
                .iter()
                .filter_map(|(id, _, _)| view.active.get(id).map(|instance| instance.node))
                .collect::<Vec<_>>();
            host.with_scene(|scene| scene.reorder_children(parent, &order))
                .map_err(|error| error.to_string())?;
        }
        Ok(transitions)
    }
}
