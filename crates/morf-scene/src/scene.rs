use animato::Update;
use morf_reactive::Graph;
use slotmap::SlotMap;
use std::collections::HashMap;
use std::time::Duration;

use crate::{animation::*, hashing::*, motion::*, motion_values::*, schema::*, types::*};

impl Scene {
    /// Creates an empty scene arena.
    pub fn new() -> Self {
        Self {
            nodes: SlotMap::with_key(),
            properties: Graph::default(),
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

    /// Advances every active behavior without invoking Lua.
    /// Sets how fast motion runs against the clock.
    ///
    /// 1 is real time. 0 is what a reduced-motion preference means: every
    /// behavior, group and spring lands on its target on the next tick, so a
    /// configuration written with motion still ends up in the same place,
    /// only without the travel.
    pub fn set_motion_scale(&mut self, scale: f64) {
        self.motion_scale = if scale.is_finite() {
            scale.max(0.0)
        } else {
            1.0
        };
    }

    /// How fast motion runs against the clock; see [`Self::set_motion_scale`].
    pub fn motion_scale(&self) -> f64 {
        self.motion_scale
    }

    /// Whether an animation's clock starts on the first tick after it was
    /// asked for, rather than being charged that tick's whole delta.
    ///
    /// A loop driven by a display wants this. There a tick's delta is the
    /// time since the previous frame, and an animation asked for between
    /// two frames did not exist for most of it -- for all of it, and then
    /// some, when the turn that asked also built a panel and held the loop.
    /// Charged the delta, a 380 ms morph whose first frame came 100 ms late
    /// is drawn a quarter of the way there on its first frame, and the eye
    /// sees it jump. Started on the tick, its first frame is its start, as
    /// it is when the shell was idle before it. Off by default: a clock
    /// driven by hand (a test, a headless run) means its deltas exactly.
    pub fn set_start_on_tick(&mut self, on: bool) {
        self.start_on_tick = on;
    }

    /// Whether a tick would move anything: an animation or group that is
    /// running rather than paused, or an ending not yet reported. What a loop
    /// asks before it keeps a clock ticking for motion.
    pub fn has_motion(&self) -> bool {
        !self.events.is_empty()
            || !self.group_events.is_empty()
            || self
                .animations
                .values()
                .any(|animation| !animation.is_paused())
            || self.groups.values().any(|group| !group.paused)
            || self.stretch_moving()
    }

    /// The animations that are running rather than paused: node, property,
    /// and whether it loops forever. For diagnostics -- what keeps an idle
    /// shell drawing.
    pub fn running_animations(&self) -> Vec<(NodeHandle, &'static str, bool)> {
        self.animations
            .iter()
            .filter(|(_, animation)| !animation.is_paused())
            .map(|(key, animation)| (NodeHandle(key.node), key.property, !animation.settles()))
            .collect()
    }

    pub fn tick_animations(&mut self, delta: Duration) -> Result<AnimationFrame, SceneError> {
        let snap = self.motion_scale == 0.0;
        // A scale of zero is a tick long enough to finish anything timed. A
        // spring is not timed, so it is landed by hand below.
        let delta = if snap {
            Duration::from_secs(1 << 20)
        } else {
            delta.mul_f64(self.motion_scale)
        };
        // The stretch springs step when the frame's positions are seen, after
        // layout; here their clock only moves, so a second surface reporting
        // on the same tick is recognised as the same moment.
        if !snap {
            self.stretch_clock += delta.as_secs_f64();
        }
        let mut frame = AnimationFrame {
            groups: self.tick_groups(delta)?,
            events: std::mem::take(&mut self.events),
            ..AnimationFrame::default()
        };
        let keys: Vec<_> = self.animations.keys().copied().collect();
        let mut finished = Vec::new();
        for key in keys {
            let animation = self
                .animations
                .get_mut(&key)
                .expect("animation key vanished");
            let paused = animation.is_paused();
            let delayed = animation.is_delayed();
            let fresh = std::mem::take(&mut animation.fresh);
            let step = if fresh && self.start_on_tick {
                0.0
            } else {
                delta.as_secs_f32()
            };
            let complete = !animation.clock.update(step);
            // A settling animation lands exactly on its target; an endless one
            // is stopped at whatever point in the cycle the clock reports.
            let value = if complete && animation.settles() {
                animation.settled().clone()
            } else {
                animation.value()
            };
            let Some(node) = self.nodes.get(key.node) else {
                finished.push(key);
                continue;
            };
            // A paused clock holds its value, and one still draining its delay
            // has not left the start value, so neither is worth a repaint.
            let idle = paused || (delayed && animation.is_delayed());
            if !idle {
                let slot = node.properties[key.property];
                if affects_layout(key.property) {
                    self.bump_layout(key.node);
                }
                self.properties.write(slot.current, value)?;
                frame.changed += 1;
            }
            if complete {
                finished.push(key);
            }
        }
        for key in finished {
            if self.animations.remove(&key).is_some() {
                self.settle_target(key)?;
            }
            frame.events.push(AnimationEvent {
                node: NodeHandle(key.node),
                property: key.property,
                end: AnimationEnd::Completed,
            });
        }
        let physics_keys: Vec<_> = self.physics.keys().copied().collect();
        let mut physics_finished = Vec::new();
        for key in physics_keys {
            if self.paused_physics.contains(&key) {
                continue;
            }
            let Some(node) = self.nodes.get(key.node) else {
                physics_finished.push(key);
                continue;
            };
            let slot = node.properties[key.property];
            if snap {
                let target = self.properties.read(slot.target)?.clone();
                if affects_layout(key.property) {
                    self.bump_layout(key.node);
                }
                self.properties.write(slot.current, target)?;
                frame.changed += 1;
                physics_finished.push(key);
                continue;
            }
            let motion = self.physics.get_mut(&key).expect("physics key vanished");
            if let PhysicsAnimation::Color { channels } = motion {
                let Value::Color(mut current) = *self.properties.read(slot.current)? else {
                    physics_finished.push(key);
                    continue;
                };
                let settled = advance_physics_color(channels, &mut current, delta);
                self.properties.write(slot.current, Value::Color(current))?;
                frame.changed += 1;
                if settled {
                    physics_finished.push(key);
                }
                continue;
            }
            let Value::Number(mut current) = *self.properties.read(slot.current)? else {
                physics_finished.push(key);
                continue;
            };
            let settled = advance_physics(motion, &mut current, delta);
            // Physics moves a property without any assignment, so this is the
            // one write that has to say so itself. Without it a paint reuses
            // the layout it already had and the scene animates behind a still
            // picture — every other path reaches here through `assign`.
            if affects_layout(key.property) {
                self.bump_layout(key.node);
            }
            self.properties
                .write(slot.current, Value::Number(current))?;
            frame.changed += 1;
            if settled {
                physics_finished.push(key);
            }
        }
        for key in physics_finished {
            self.physics.remove(&key);
            self.paused_physics.remove(&key);
            self.settle_target(key)?;
            frame.events.push(AnimationEvent {
                node: NodeHandle(key.node),
                property: key.property,
                end: AnimationEnd::Completed,
            });
        }
        let report = self.properties.flush()?;
        if let Some(error) = report.errors.first() {
            return Err(SceneError::Reactive(format!(
                "{}: {}",
                error.effect, error.message
            )));
        }
        frame.exited = self.finished_exits();
        frame.active = !self.animations.is_empty()
            || !self.physics.is_empty()
            || !self.groups.is_empty()
            || self.stretch_moving();
        Ok(frame)
    }
}
