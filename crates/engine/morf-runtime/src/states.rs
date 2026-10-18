//! Declarative states: what a configuration declares, and what the runtime
//! keeps while moving between them; and what a running binding captured.

use std::collections::{HashMap, HashSet};

use morf_scene::reactive::SignalId;
use morf_scene::{Behavior, NodeHandle, Value as SceneValue};

use crate::Handler;
use morf_value::IpcValue;

#[derive(Clone)]
pub struct StateDefinition {
    pub properties: Vec<(String, StateValue)>,
    pub anchors: Option<std::collections::BTreeMap<String, SceneValue>>,
    pub parent: Option<NodeHandle>,
    /// A binding that, when true, selects this state on its own.
    pub when: Option<Handler>,
    /// Which `when` is asked first: lowest first, ties by name.
    pub order: f64,
}

#[derive(Clone)]
pub enum StateValue {
    Value(SceneValue),
    Binding(Handler),
}

#[derive(Clone)]
pub struct StateTransition {
    pub from: String,
    pub to: String,
    pub reversible: bool,
    pub behavior: Behavior,
}

#[derive(Default)]
pub struct StateSet {
    pub definitions: HashMap<String, StateDefinition>,
    pub transitions: Vec<StateTransition>,
    pub current: Option<String>,
}

/// Each node's declared states, and which it is in.
#[derive(Default)]
pub struct States {
    sets: HashMap<NodeHandle, StateSet>,
}

impl States {
    /// Gives `node` its states, replacing any it had.
    pub fn declare(&mut self, node: NodeHandle, set: StateSet) {
        self.sets.insert(node, set);
    }

    /// `node`'s states, if it declared any.
    pub fn get(&self, node: NodeHandle) -> Option<&StateSet> {
        self.sets.get(&node)
    }

    /// Marks `node` as in state `name`. False when it has no states.
    pub fn enter(&mut self, node: NodeHandle, name: &str) -> bool {
        let Some(set) = self.sets.get_mut(&node) else {
            return false;
        };
        set.current = Some(name.to_owned());
        true
    }

    /// Forgets a removed node's states.
    pub fn forget(&mut self, node: &NodeHandle) {
        self.sets.remove(node);
    }
}

#[derive(Default)]
pub struct Capture {
    pub reads: HashSet<SignalId>,
    pub property_reads: HashSet<(NodeHandle, String, bool)>,
    pub writes: Vec<(SignalId, IpcValue)>,
    /// List models read whose revision signal does not exist yet.
    pub model_reads: Vec<std::rc::Rc<std::cell::RefCell<morf_scene::ListModel>>>,
}
