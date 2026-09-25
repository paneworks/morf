//! Leaving the tree slowly: a node's exit animation.
//!
//! A node may say how it leaves: `exit = { opacity = 0, scale = 0.9 }` with a
//! duration and an easing. When whatever owns it lets it go -- a `Loader`
//! turning inactive, a list row removed, `ui.destroy` -- the owner asks the
//! scene to [`Scene::begin_exit`] instead of removing it. The node then stays
//! in the tree and keeps being drawn while its properties animate to those
//! values, but it is out of the layout's flow (the siblings it was pushing
//! close up at once; it keeps the box it had, relative to its parent) and it
//! takes no input. When every exit animation has ended the scene reports the
//! node in [`AnimationFrame::exited`] and its owner removes it for good.
//!
//! Put back before that -- the row returns, the Loader turns active again --
//! the owner calls [`Scene::cancel_exit`]: the node rejoins the flow and its
//! properties animate back to what they were when it started to leave.

use std::cell::Cell;

use crate::{animation::*, motion::*, schema::*, types::*};

/// How a node leaves: where each property goes, and how it gets there.
#[derive(Clone, Debug, PartialEq)]
pub struct ExitSpec {
    /// Each property the exit moves, and the value it moves it to.
    pub values: Vec<(String, Value)>,
    /// How every one of them travels.
    pub behavior: Behavior,
}

/// A node on its way out.
#[derive(Debug)]
pub(crate) struct Exiting {
    /// What each property the exit moves was aimed at when it started, so a
    /// node put back can go back there.
    restore: Vec<(&'static str, Value)>,
    behavior: Behavior,
    /// `x` and `y` when it started: the box it keeps moves by however far
    /// those have moved since, so an exit can slide it.
    origin: (f64, f64),
    /// The box it keeps, relative to its parent's, as the first layout to
    /// see it leaving found it: `[x, y, width, height]`. Set by the layout
    /// through a shared reference, which is what the cell is for.
    frame: Cell<Option<[f64; 4]>>,
    /// Reported as done: its animations ended and its owner was told.
    done: bool,
}

impl Scene {
    /// Declares how a node leaves, or that it simply goes (`None`).
    ///
    /// Every property named must be one the node has, and each value must be
    /// one it takes; checked here, so a misspelt exit is an error where it
    /// is written rather than a node that vanishes without its animation.
    pub fn set_exit(&mut self, node: NodeHandle, exit: Option<ExitSpec>) -> Result<(), SceneError> {
        let id = self.live(node)?;
        let Some(mut exit) = exit else {
            self.exit_specs.remove(&id);
            self.exit_placed.remove(&id);
            return Ok(());
        };
        let element = self.nodes[id].element;
        for (property, value) in &mut exit.values {
            let slot = *self.nodes[id]
                .properties
                .get(property.as_str())
                .ok_or_else(|| SceneError::UnknownProperty {
                    element: element.name(),
                    property: property.clone(),
                })?;
            *value = coerce(element, property, slot.kind, value.clone())?;
        }
        self.exit_specs.insert(id, exit);
        self.exit_placed.entry(id).or_default();
        Ok(())
    }

    /// Whether any node has declared an exit: a layout with none to watch
    /// for notes nothing.
    pub fn has_exits(&self) -> bool {
        !self.exit_placed.is_empty()
    }

    /// Notes where a node that declared an exit was placed, relative to its
    /// parent's box: where it stays if it starts to leave. For the layout.
    pub fn note_placed(&self, node: NodeHandle, frame: [f64; 4]) {
        if let Some(placed) = self.exit_placed.get(&node.id()) {
            placed.set(Some(frame));
        }
    }

    /// How a node was declared to leave, if it was.
    pub fn exit_spec(&self, node: NodeHandle) -> Option<&ExitSpec> {
        self.exit_specs.get(&node.id())
    }

    /// Starts a node on its way out, if it has an exit that takes any time.
    ///
    /// `true` means it is leaving -- now, or already -- and its owner waits
    /// for [`AnimationFrame::exited`] before removing it; `false` that it has
    /// no exit to run, and its owner removes it at once as it always did.
    pub fn begin_exit(&mut self, node: NodeHandle) -> Result<bool, SceneError> {
        let id = self.live(node)?;
        if self.exiting.contains_key(&id) {
            return Ok(true);
        }
        let Some(spec) = self.exit_specs.get(&id).cloned() else {
            return Ok(false);
        };
        if !spec.behavior.intercepts() || spec.values.is_empty() {
            return Ok(false);
        }
        let mut restore = Vec::with_capacity(spec.values.len());
        for (property, to) in &spec.values {
            let (name, slot) = self.nodes[id]
                .properties
                .get_key_value(property.as_str())
                .map(|(name, slot)| (*name, *slot))
                .expect("checked when the exit was declared");
            let target = self.properties.read(slot.target)?.clone();
            let from = self.properties.read(slot.current)?.clone();
            restore.push((name, target));
            if interpolatable(&from, to) {
                self.animate_from(node, name, from, to.clone(), spec.behavior)?;
            } else {
                // Nothing between the two: it takes the value when it leaves.
                self.assign(node, name, to.clone())?;
            }
        }
        let origin = (self.number(node, "x")?, self.number(node, "y")?);
        self.exiting.insert(
            id,
            Exiting {
                restore,
                behavior: spec.behavior,
                origin,
                // Where the last layout put it, if one has: every layout
                // after keeps it there, a fresh one included.
                frame: Cell::new(self.exit_placed.get(&id).and_then(Cell::get)),
                done: false,
            },
        );
        // Out of its parent's flow: the parent measures and places again.
        self.bump_layout(id);
        Ok(true)
    }

    /// Takes a node that was leaving back: it rejoins the layout and every
    /// property its exit moved goes back to where it was aimed before, by
    /// the exit's own timing. `false` if it was not leaving.
    pub fn cancel_exit(&mut self, node: NodeHandle) -> Result<bool, SceneError> {
        let id = self.live(node)?;
        let Some(exiting) = self.exiting.remove(&id) else {
            return Ok(false);
        };
        for (name, back) in exiting.restore {
            let slot = self.nodes[id].properties[name];
            let from = self.properties.read(slot.current)?.clone();
            if interpolatable(&from, &back) {
                self.animate_from(node, name, from, back, exiting.behavior)?;
            } else {
                self.assign(node, name, back)?;
            }
        }
        self.bump_layout(id);
        Ok(true)
    }

    /// Whether a node is on its way out: drawn, but out of the layout's flow
    /// and taking no input.
    pub fn is_exiting(&self, node: NodeHandle) -> bool {
        self.exiting.contains_key(&node.id())
    }

    /// Every node on its way out.
    pub fn exiting_nodes(&self) -> impl Iterator<Item = NodeHandle> + '_ {
        self.exiting.keys().map(|id| NodeHandle(*id))
    }

    /// The box a leaving node keeps, relative to its parent's box, once a
    /// layout has fixed it: `[x, y, width, height]`, moved by however far
    /// its `x` and `y` have moved since it started to leave.
    pub fn exit_frame(&self, node: NodeHandle) -> Option<[f64; 4]> {
        let exiting = self.exiting.get(&node.id())?;
        let [x, y, width, height] = exiting.frame.get()?;
        let dx = self.number(node, "x").unwrap_or(exiting.origin.0) - exiting.origin.0;
        let dy = self.number(node, "y").unwrap_or(exiting.origin.1) - exiting.origin.1;
        Some([x + dx, y + dy, width, height])
    }

    /// Fixes the box a leaving node keeps, relative to its parent's, if no
    /// layout has yet: the first to see it leaving decides, from where it
    /// last placed it, and every layout after keeps it there.
    pub fn fix_exit_frame(&self, node: NodeHandle, frame: [f64; 4]) {
        if let Some(exiting) = self.exiting.get(&node.id())
            && exiting.frame.get().is_none()
        {
            exiting.frame.set(Some(frame));
        }
    }

    /// Nodes whose exit has ended, each reported once: nothing the exit
    /// started is still moving. Their owners remove them now.
    pub(crate) fn finished_exits(&mut self) -> Vec<NodeHandle> {
        let mut finished = Vec::new();
        for (id, exiting) in &mut self.exiting {
            if exiting.done {
                continue;
            }
            let moving = exiting.restore.iter().any(|(name, _)| {
                let key = PropertyKey {
                    node: *id,
                    property: name,
                };
                self.animations.contains_key(&key) || self.physics.contains_key(&key)
            });
            if !moving {
                exiting.done = true;
                finished.push(NodeHandle(*id));
            }
        }
        finished
    }
}
