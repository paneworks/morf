//! What the engine does when the scene loses a subtree, a node starts or
//! stops leaving, the graph's garbage is collected, and a configuration
//! keeps a value across reloads.

use std::collections::HashSet;

use morf_scene::NodeHandle;
use morf_scene::reactive::SignalId;
use morf_value::IpcValue;

use super::Engine;

/// One part of a scoped id: 1..256 bytes, no empty dotted segment.
pub fn validate_scope_part(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 256 {
        return Err("scope IDs must be 1..256 bytes".into());
    }
    if value.starts_with('.') || value.ends_with('.') || value.contains("..") {
        return Err("scope IDs cannot contain empty segments".into());
    }
    Ok(())
}

/// `prefix.name`, each part checked, the whole at most 256 bytes.
pub fn scoped_id(prefix: &str, name: &str) -> Result<String, String> {
    validate_scope_part(prefix)?;
    validate_scope_part(name)?;
    let value = format!("{prefix}.{name}");
    if value.len() > 256 {
        return Err("scoped reloadable ID exceeds 256 bytes".into());
    }
    Ok(value)
}

impl Engine {
    /// Hands the graph what removed nodes left behind, when it is here to
    /// take them; while a flush holds it they wait for the next call.
    pub fn collect_graph_garbage(&mut self) {
        if self.reactive.flushing {
            return;
        }
        self.model_revisions.collect_dead(&mut self.reactive);
        self.reactive.collect_garbage();
    }

    /// Removes `node` and its subtree from the scene and from every
    /// subsystem that kept something for one of them. The `on_destroyed`
    /// hooks are queued (deepest first) for when handlers can run. Returns
    /// the nodes removed, for what the scripting layer keeps per node too;
    /// nothing when `node` was already gone.
    pub fn remove_subtree(&mut self, node: NodeHandle) -> Option<HashSet<NodeHandle>> {
        let mut nodes = vec![node];
        let mut index = 0;
        while index < nodes.len() {
            let children = self.scene.children(nodes[index]).unwrap_or_default();
            nodes.extend_from_slice(children);
            index += 1;
        }
        self.revisions.scene_revision = self.revisions.scene_revision.wrapping_add(1);
        if self.scene.remove(node).is_err() {
            return None;
        }
        // Deepest first, so a child lets go of what it holds before the
        // parent that may have lent it.
        for removed in nodes.iter().rev() {
            if let Some(hook) = self.destroy_hooks.remove(removed) {
                self.pending_destroyed.push(hook);
            }
        }
        let removed = nodes.into_iter().collect::<HashSet<_>>();
        for node in &removed {
            self.retained.forget(node);
            self.states.remove(node);
            self.views.remove(node);
            self.timer_callbacks.remove(node);
            self.linked_texts.remove(node);
            self.shortcuts.remove(node);
        }
        // A removed field cannot keep the keyboard. The node that had focus
        // is handed on by the focus check, which still needs to know it went.
        self.editing.forget(&removed);
        self.focus
            .memory
            .retain(|scope, node| !removed.contains(scope) && !removed.contains(node));
        self.animation.forget(&removed);
        self.events.forget(&removed);
        self.timers.retain_nodes(|node| !removed.contains(&node));
        // Bindings that drive a removed node, and the signals that tracked
        // its properties' reads: the graph forgets both, or every one of them
        // keeps re-running and growing for the life of the shell.
        self.reactive.forget_effects_of(&removed);
        let dead_signals = self
            .property_signals
            .iter()
            .filter(|((node, _, _), _)| removed.contains(node))
            .map(|(_, signal)| *signal)
            .collect::<HashSet<_>>();
        self.reactive.forget_signals(dead_signals);
        self.collect_graph_garbage();
        self.property_signals
            .retain(|(node, _, _), _| !removed.contains(node));
        self.current_property_names
            .retain(|_, (node, _)| !removed.contains(node));
        let windows = &mut self.windows;
        windows.window_surfaces_changed |= crate::layout::forget_windows_of(
            &removed,
            &mut windows.window_surfaces,
            &mut windows.popup_node_anchors,
        );
        Some(removed)
    }

    /// Starts a node on its way out, if it declared an `exit`: it stays in
    /// the tree, drawn and out of the flow, held until the exit ends. `false`
    /// when it has no exit to play, and whoever let go of it removes it.
    pub fn begin_node_exit(&mut self, node: NodeHandle) -> bool {
        let start = crate::animation::exits::begin_exit(
            &mut self.scene,
            &mut self.retained.retention,
            &mut self.animation,
            node,
        );
        if start == crate::animation::exits::ExitStart::Started {
            self.revisions.scene_revision = self.revisions.scene_revision.wrapping_add(1);
        }
        start.leaving()
    }

    /// Takes back a node that was on its way out. `false` if it was not
    /// leaving.
    pub fn cancel_node_exit(&mut self, node: NodeHandle) -> bool {
        if !crate::animation::exits::cancel_exit(
            &mut self.scene,
            &mut self.retained.retention,
            &mut self.animation,
            node,
        ) {
            return false;
        }
        self.revisions.scene_revision = self.revisions.scene_revision.wrapping_add(1);
        true
    }

    /// A value kept across reloads by `name`: its signal, starting from what
    /// the last configuration left (when it is the same kind of value) or
    /// `initial`, and whether it was restored.
    pub fn register_reloadable(
        &mut self,
        name: String,
        initial: IpcValue,
    ) -> Result<(SignalId, bool), String> {
        // One rule for every way in: what counts as a legal name used to
        // depend on which door you came through.
        validate_scope_part(&name)?;
        if self.reloadable.contains_key(&name) {
            return Err(format!("reloadable id `{name}` is already registered"));
        }
        let mut restored = false;
        let value = match self.reload_seed.remove(&name) {
            Some(value) if std::mem::discriminant(&value) == std::mem::discriminant(&initial) => {
                restored = true;
                value
            }
            Some(_) => {
                self.log(
                    crate::log::LogLevel::Warn,
                    format!("reloadable `{name}` changed value type; using its new default"),
                );
                initial
            }
            None => initial,
        };
        let id = self
            .reactive
            .graph
            .as_mut()
            .ok_or_else(|| "reactive graph is already running".to_owned())?
            .signal(format!("reloadable.{name}"), value.clone());
        self.reactive.values.insert(id, value);
        self.reactive.signals.push(id);
        self.reloadable.insert(name, id);
        Ok((id, restored))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::{HandlerId, HandlerRegistry};
    use morf_scene::Element;

    struct Nowhere;

    impl HandlerRegistry for Nowhere {
        fn release(&self, _id: HandlerId) {}
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn removing_a_subtree_forgets_it_everywhere_and_queues_its_hooks() {
        let mut engine = Engine::new();
        let root = engine.scene.create(Element::Item);
        let child = engine.scene.create(Element::Rect);
        engine.scene.reparent(child, Some(root)).unwrap();
        engine.linked_texts.insert(child);
        let hook = crate::Handler::new(HandlerId(1), std::rc::Rc::new(Nowhere));
        engine.destroy_hooks.insert(child, hook);
        let removed = engine.remove_subtree(root).expect("it was there");
        assert_eq!(removed.len(), 2);
        assert!(engine.linked_texts.is_empty());
        assert_eq!(engine.pending_destroyed.len(), 1);
        assert!(engine.remove_subtree(root).is_none());
    }

    #[test]
    fn a_reloadable_value_comes_back_only_as_the_same_kind() {
        let mut engine = Engine::new();
        engine
            .reload_seed
            .insert("a.b".into(), IpcValue::Integer(7));
        let (_, restored) = engine
            .register_reloadable("a.b".into(), IpcValue::Integer(1))
            .unwrap();
        assert!(restored);
        assert!(
            engine
                .register_reloadable("a.b".into(), IpcValue::Nil)
                .is_err()
        );
        engine
            .reload_seed
            .insert("c".into(), IpcValue::Boolean(true));
        let (_, restored) = engine
            .register_reloadable("c".into(), IpcValue::Integer(1))
            .unwrap();
        assert!(!restored);
        assert_eq!(scoped_id("x", "y").unwrap(), "x.y");
        assert!(scoped_id("x", "").is_err());
    }
}
