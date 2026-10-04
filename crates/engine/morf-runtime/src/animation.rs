//! Motion: what a frame of animation does beyond the scene's own tick.
//!
//! The scene interpolates every property itself ([`morf_scene::Scene::tick_animations`]).
//! What is kept here is the rest of a frame's motion: the `on_finished`
//! handlers behaviours and groups registered, the loops a node keeps up on
//! its own (`loop`), properties that follow another node's (`ui.follow`),
//! theme colours easing to a new value, nodes on their way out (exits), and
//! the settings behaviours and flings are built from.

pub mod behaviors;
pub mod easing;
pub mod exits;
pub mod fades;
pub mod fling;
pub mod follow;
pub mod loops;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap, HashSet};

use morf_scene::{AnimationEnd, AnimationFrame, GroupId, NodeHandle, Scene, Value};
use morf_value::IpcValue;

use crate::handler::{Handler, Handlers};

pub use fades::ThemeFade;
pub use follow::Follow;
pub use loops::RunningLoop;

/// Every piece of motion state a runtime keeps beside its scene.
#[derive(Default)]
pub struct Animation {
    /// A behaviour's `on_finished`, by the node and property it is on.
    pub callbacks: HashMap<(NodeHandle, String), Handler>,
    /// An animation group's `on_finished`; fired once and dropped.
    pub groups: HashMap<GroupId, Handler>,
    /// The loops each node is running, by property.
    pub loops: HashMap<NodeHandle, BTreeMap<String, RunningLoop>>,
    /// Properties tied to another node's (`ui.follow`), applied every tick.
    pub follows: Vec<Follow>,
    /// Theme colours on their way to the value last written to them.
    pub fades: Vec<ThemeFade>,
    /// Nodes registered with retention only for their exit, so taking the
    /// exit back unregisters them rather than unlocking someone else's hold.
    pub exit_registered: HashSet<NodeHandle>,
}

/// What a runtime's motion asks of the state it lives in.
pub trait AnimationHost {
    fn animation(&mut self) -> &mut Animation;
    fn scene(&mut self) -> &mut Scene;
    /// Writes a node's property the way any write does: through its
    /// behaviour, with the bindings that read it told.
    fn assign(&mut self, node: NodeHandle, property: &str, value: Value) -> Result<(), String>;
}

/// One `on_finished` handler a frame owes, with what it is called with.
pub struct Finished {
    pub handler: Handler,
    pub args: Vec<IpcValue>,
    /// Who registered it, for a warning when it fails.
    pub source: &'static str,
}

impl Animation {
    /// The handlers the animations and groups that ended in `frame` registered,
    /// in the order they ended. A group's handler is taken out as it is
    /// collected: it fires once.
    pub fn finished(&mut self, frame: &AnimationFrame) -> Vec<Finished> {
        let mut finished = frame
            .events
            .iter()
            .filter_map(|event| {
                let handler = self
                    .callbacks
                    .get(&(event.node, event.property.to_owned()))?
                    .clone();
                let mut args = vec![IpcValue::String(end_name(event.end).to_owned())];
                if !event.property.is_empty() {
                    args.insert(0, IpcValue::String(event.property.to_owned()));
                }
                Some(Finished {
                    handler,
                    args,
                    source: "behavior",
                })
            })
            .collect::<Vec<_>>();
        for event in &frame.groups {
            if let Some(handler) = self.groups.remove(&event.group) {
                finished.push(Finished {
                    handler,
                    args: vec![IpcValue::String(end_name(event.end).to_owned())],
                    source: "animation group",
                });
            }
        }
        finished
    }

    /// Lets go of everything kept for nodes that were removed.
    pub fn forget(&mut self, removed: &HashSet<NodeHandle>) {
        for node in removed {
            self.loops.remove(node);
            self.exit_registered.remove(node);
        }
        self.callbacks
            .retain(|(owner, _), _| !removed.contains(owner));
    }

    /// Whether any theme colour is still easing.
    pub fn fading(&self) -> bool {
        !self.fades.is_empty()
    }
}

/// Calls each owed `on_finished` handler. A failing one is reported, not
/// allowed to stop the frame: the warnings come back, one per failure.
pub fn report_finished(handlers: &mut dyn Handlers, finished: Vec<Finished>) -> Vec<String> {
    let mut warnings = Vec::new();
    for Finished {
        handler,
        args,
        source,
    } in finished
    {
        if let Err(message) = handlers.notify(&handler, &args) {
            warnings.push(format!("{source} on_finished: {message}"));
        }
    }
    warnings
}

/// The name a handler is told the reason an animation ended by.
pub fn end_name(end: AnimationEnd) -> &'static str {
    match end {
        AnimationEnd::Completed => "completed",
        AnimationEnd::Stopped => "stopped",
        AnimationEnd::Canceled => "canceled",
    }
}
