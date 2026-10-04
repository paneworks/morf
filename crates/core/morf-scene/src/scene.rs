use crate::property_store::PropertyStore;
use slotmap::SlotMap;
use std::collections::HashMap;

use crate::{animation::*, hashing::*, motion::*, motion_values::*, schema::*, types::*};

impl Scene {
    /// Creates an empty scene arena.
    pub fn new() -> Self {
        Self {
            nodes: SlotMap::with_key(),
            properties: PropertyStore::default(),
            behaviors: FastMap::default(),
            animations: FastMap::default(),
            physics: FastMap::default(),
            physics_specs: FastMap::default(),
            paused_physics: FastSet::default(),
            events: Vec::new(),
            groups: HashMap::new(),
            group_events: Vec::new(),
            next_group: 0,
            layout_revision: 0,
            root_revisions: FastMap::default(),
            detached_revisions: FastMap::default(),
            motion_scale: 1.0,
            start_on_tick: false,
            removed: Vec::new(),
            shaders: FastMap::default(),
            terminal_screens: FastMap::default(),
            tracks: FastMap::default(),
            masks: FastMap::default(),
            mask_owners: FastMap::default(),
            stretch: FastMap::default(),
            stretch_clock: 0.0,
            exit_specs: FastMap::default(),
            exit_placed: FastMap::default(),
            exiting: FastMap::default(),
        }
    }

    /// Allocates an element with every schema property initialized.
    pub fn create(&mut self, element: Element) -> NodeHandle {
        let node = self.nodes.insert_with_key(|_| {
            let properties = schema(element)
                .into_iter()
                .map(|spec| {
                    // Named by the property alone: nothing subscribes to
                    // these signals, so the name is never shown, and
                    // formatting one per signal -- element, node, property,
                    // level -- was most of what building a node cost.
                    let current = self.properties.signal(spec.name, spec.default.clone());
                    let target = self.properties.signal(spec.name, spec.default);
                    (
                        spec.name,
                        PropertySlot {
                            current,
                            target,
                            kind: spec.kind,
                        },
                    )
                })
                .collect();
            Node {
                element,
                parent: None,
                children: Vec::new(),
                properties,
                stamps: LayoutStamps::default(),
            }
        });
        // A new node is a tree of its own until it is given a parent.
        self.bump_layout(node);
        NodeHandle(node)
    }

    /// How many nodes are live.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// How many property signals the scene holds, live nodes' only.
    pub fn property_signal_count(&self) -> usize {
        self.properties.signal_count()
    }

    /// Returns whether a handle still refers to a live node generation.
    pub fn contains(&self, node: NodeHandle) -> bool {
        self.nodes.contains_key(node.0)
    }

    /// Returns all live nodes without a parent in arena order.
    pub fn roots(&self) -> Vec<NodeHandle> {
        self.nodes
            .iter()
            .filter(|(_, node)| node.parent.is_none())
            .map(|(id, _)| NodeHandle(id))
            .collect()
    }

    /// Returns the element kind for a live node.
    pub fn element(&self, node: NodeHandle) -> Result<Element, SceneError> {
        Ok(self.nodes[self.live(node)?].element)
    }

    /// Checks whether a live element schema declares a property.
    pub fn has_property(&self, node: NodeHandle, property: &str) -> Result<bool, SceneError> {
        Ok(self.nodes[self.live(node)?]
            .properties
            .contains_key(property))
    }

    /// Reports whether a property currently advances on animation ticks.
    pub fn is_animating(&self, node: NodeHandle, property: &str) -> Result<bool, SceneError> {
        let id = self.live(node)?;
        let (name, _) = self.nodes[id]
            .properties
            .get_key_value(property)
            .ok_or_else(|| SceneError::UnknownProperty {
                element: self.nodes[id].element.name(),
                property: property.to_owned(),
            })?;
        let key = PropertyKey {
            node: id,
            property: name,
        };
        Ok(self.animations.contains_key(&key) || self.physics.contains_key(&key))
    }

    /// Appends a node to a new parent while preserving the child's identity.
    pub fn reparent(
        &mut self,
        child: NodeHandle,
        parent: Option<NodeHandle>,
    ) -> Result<(), SceneError> {
        let child_id = self.live(child)?;
        let parent_id = parent.map(|handle| self.live(handle)).transpose()?;
        if parent_id == Some(child_id) {
            return Err(SceneError::ParentCycle);
        }
        let mut ancestor = parent_id;
        while let Some(node) = ancestor {
            if node == child_id {
                return Err(SceneError::ParentCycle);
            }
            ancestor = self.nodes[node].parent;
        }

        // Both trees change: the one it left and the one it joined. Before
        // the move, the tree it is in; after, the tree it is in now.
        self.bump_layout(child_id);
        if let Some(old_parent) = self.nodes[child_id].parent {
            self.nodes[old_parent]
                .children
                .retain(|node| node.id() != child_id);
            // The parent it left has one child fewer, and the tree lost a
            // node its last layout placed.
            self.bump_layout(old_parent);
            self.mark_detached(old_parent);
        }
        // A mask moved anywhere but under the node it masks stops masking.
        if let Some(owner) = self.mask_owners.get(&child_id).copied()
            && parent_id != Some(owner.id())
        {
            self.mask_owners.remove(&child_id);
            self.masks.remove(&owner.id());
        }
        self.nodes[child_id].parent = parent_id;
        if let Some(parent) = parent_id {
            self.nodes[parent].children.push(child);
            // No longer a root, so no longer a tree with a revision of its own.
            self.root_revisions.remove(&child_id);
            self.detached_revisions.remove(&child_id);
            self.bump_layout(parent);
        }
        self.bump_layout(child_id);
        self.nodes[child_id].stamps.attached = self.layout_revision;
        Ok(())
    }

    /// Puts `order` first among a parent's children, in that order.
    ///
    /// Children not named keep their relative order after the named ones.
    /// Handles that are not this parent's children are ignored, so a stale
    /// list is harmless. What a reconciled view uses to make paint order and
    /// positioner order follow the model's order.
    pub fn reorder_children(
        &mut self,
        parent: NodeHandle,
        order: &[NodeHandle],
    ) -> Result<(), SceneError> {
        let parent_id = self.live(parent)?;
        let current = std::mem::take(&mut self.nodes[parent_id].children);
        let mut arranged = Vec::with_capacity(current.len());
        for wanted in order {
            if current.contains(wanted) && !arranged.contains(wanted) {
                arranged.push(*wanted);
            }
        }
        let changed = arranged.iter().zip(current.iter()).any(|(a, b)| a != b);
        for child in current {
            if !arranged.contains(&child) {
                arranged.push(child);
            }
        }
        if changed {
            self.bump_layout(parent_id);
        }
        self.nodes[parent_id].children = arranged;
        Ok(())
    }

    /// Returns the current parent handle.
    pub fn parent(&self, node: NodeHandle) -> Result<Option<NodeHandle>, SceneError> {
        Ok(self.nodes[self.live(node)?].parent.map(NodeHandle))
    }

    /// Returns child handles in tree order.
    ///
    /// Tree order is what positioners lay out by and what key focus walks.
    /// Paint and hit testing use [`Scene::paint_order`], which is this order
    /// with `z` applied.
    pub fn children(&self, node: NodeHandle) -> Result<&[NodeHandle], SceneError> {
        Ok(&self.nodes[self.live(node)?].children)
    }

    /// Returns child handles in paint order: tree order, stably sorted by
    /// `z`, so a child with a higher `z` paints over, and is hit before, its
    /// siblings whatever its place in the tree.
    ///
    /// Borrowed when no child sets `z`, which is nearly always.
    pub fn paint_order(
        &self,
        node: NodeHandle,
    ) -> Result<std::borrow::Cow<'_, [NodeHandle]>, SceneError> {
        let children = self.children(node)?;
        // A mask is drawn only as its owner's mask, and hit never.
        if !self.mask_owners.is_empty() && children.iter().any(|child| self.is_mask(*child)) {
            let z = |child: &NodeHandle| match self.current(*child, "z") {
                Ok(Value::Number(z)) => *z,
                _ => 0.0,
            };
            let mut sorted: Vec<NodeHandle> = children
                .iter()
                .copied()
                .filter(|child| !self.is_mask(*child))
                .collect();
            sorted.sort_by(|a, b| z(a).total_cmp(&z(b)));
            return Ok(std::borrow::Cow::Owned(sorted));
        }
        let z = |child: &NodeHandle| match self.current(*child, "z") {
            Ok(Value::Number(z)) => *z,
            _ => 0.0,
        };
        if children.iter().all(|child| z(child) == 0.0) {
            return Ok(std::borrow::Cow::Borrowed(children));
        }
        let mut sorted = children.to_vec();
        sorted.sort_by(|a, b| z(a).total_cmp(&z(b)));
        Ok(std::borrow::Cow::Owned(sorted))
    }

    /// Removes a node and all descendants, invalidating their handles.
    pub fn remove(&mut self, node: NodeHandle) -> Result<(), SceneError> {
        let id = self.live(node)?;
        self.bump_layout(id);
        if let Some(parent) = self.nodes[id].parent {
            self.nodes[parent]
                .children
                .retain(|handle| handle.id() != id);
            self.bump_layout(parent);
            self.mark_detached(parent);
        }
        let mut pending = vec![id];
        while let Some(current) = pending.pop() {
            pending.extend(self.nodes[current].children.iter().map(|child| child.id()));
            self.removed.push(NodeHandle(current));
            self.root_revisions.remove(&current);
            self.detached_revisions.remove(&current);
            self.terminal_screens.remove(&current);
            self.tracks.remove(&current);
            self.masks.remove(&current);
            if let Some(owner) = self.mask_owners.remove(&current) {
                self.masks.remove(&owner.id());
            }
            self.stretch.remove(&current);
            self.exit_specs.remove(&current);
            self.exit_placed.remove(&current);
            self.exiting.remove(&current);
            // Its properties live in the scene's signal graph, not in the
            // node; they go with it or they stay allocated for the life of
            // the process, two per property per node ever made.
            if let Some(gone) = self.nodes.remove(current) {
                for slot in gone.properties.values() {
                    self.properties.remove_signal(slot.current);
                    self.properties.remove_signal(slot.target);
                }
            }
        }
        // Once for the whole subtree, not once a node: a panel of a thousand
        // nodes let go with a few hundred behaviours was a few hundred
        // thousand key comparisons, and several milliseconds of the turn
        // that closed it.
        let nodes = &self.nodes;
        self.behaviors.retain(|key, _| nodes.contains_key(key.node));
        self.animations
            .retain(|key, _| nodes.contains_key(key.node));
        self.physics.retain(|key, _| nodes.contains_key(key.node));
        self.physics_specs
            .retain(|key, _| nodes.contains_key(key.node));
        self.paused_physics
            .retain(|key| nodes.contains_key(key.node));
        self.retain_live_groups();
        Ok(())
    }

    /// Assigns and coerces a plain value to both target and rendered property levels.
    pub fn assign(
        &mut self,
        node: NodeHandle,
        property: &str,
        value: impl Into<Value>,
    ) -> Result<(), SceneError> {
        let id = self.live(node)?;
        let element = self.nodes[id].element;
        let (property_name, slot) = self.nodes[id]
            .properties
            .get_key_value(property)
            .map(|(name, slot)| (*name, *slot))
            .ok_or_else(|| SceneError::UnknownProperty {
                element: element.name(),
                property: property.to_owned(),
            })?;
        let value = coerce(element, property, slot.kind, value.into())?;
        let key = PropertyKey {
            node: id,
            property: property_name,
        };
        if self.properties.read(slot.target)? == &value {
            return Ok(());
        }
        if let Some(spec) = self.physics_specs.get(&key).copied()
            && let Value::Color(target) = value
            && let Value::Color(current) = *self.properties.read(slot.current)?
        {
            let velocity = self
                .physics
                .get(&key)
                .map_or([0.0; 4], PhysicsAnimation::color_velocity);
            self.animations.remove(&key);
            self.properties.write(slot.target, Value::Color(target))?;
            self.physics.insert(
                key,
                physics_animation_color(current, target, velocity, spec),
            );
        } else if let Some(spec) = self.physics_specs.get(&key).copied()
            && let Value::Number(target) = value
            && matches!(self.properties.read(slot.current)?, Value::Number(_))
        {
            let velocity = self
                .physics
                .get(&key)
                .map_or(0.0, PhysicsAnimation::velocity);
            let current = self.properties.read(slot.current)?.clone();
            let Value::Number(current) = current else {
                unreachable!("numeric physics target had a non-numeric current value")
            };
            self.animations.remove(&key);
            self.properties.write(slot.target, Value::Number(target))?;
            self.physics
                .insert(key, physics_animation(current, target, velocity, spec));
        } else if let Some(behavior) = self.behaviors.get(&key).copied()
            && behavior.intercepts()
            && interpolatable(self.properties.read(slot.current)?, &value)
        {
            let from = animation_start(
                property_name,
                self.properties.read(slot.current)?.clone(),
                &value,
                behavior.rotation_direction,
            );
            let initial_velocity = self
                .animations
                .get(&key)
                .filter(|_| behavior.keep_velocity)
                .map(Animation::velocity)
                .unwrap_or_else(|| zero_velocity(&from));
            self.properties.write(slot.target, value.clone())?;
            self.animations.insert(
                key,
                Animation::new(
                    from,
                    value,
                    initial_velocity,
                    initial_velocity.is_moving(),
                    behavior,
                ),
            );
        } else {
            let interrupted =
                self.animations.remove(&key).is_some() | self.physics.remove(&key).is_some();
            self.paused_physics.remove(&key);
            if interrupted {
                self.push_event(key, AnimationEnd::Canceled);
            }
            self.properties.batch(|graph| {
                graph.write(slot.target, value.clone())?;
                graph.write(slot.current, value)?;
                Ok(())
            })?;
        }
        // Conservative: an assignment that only sets an animation's target has
        // not moved anything yet, but the ticks that follow will, and one extra
        // layout pass is a great deal cheaper than a frame drawn at stale
        // geometry.
        self.touch_layout(id, property_name);
        Ok(())
    }
}
