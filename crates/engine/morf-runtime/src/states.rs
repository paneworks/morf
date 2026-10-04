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

#[derive(Default)]
pub struct Capture {
    pub reads: HashSet<SignalId>,
    pub property_reads: HashSet<(NodeHandle, String, bool)>,
    pub writes: Vec<(SignalId, IpcValue)>,
    /// List models read whose revision signal does not exist yet.
    pub model_reads: Vec<std::rc::Rc<std::cell::RefCell<morf_scene::ListModel>>>,
}
