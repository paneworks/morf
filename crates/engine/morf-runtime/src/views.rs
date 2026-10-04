//! Views that follow a list model: a `ListView` or a `GridView` that builds
//! delegates only for the rows in sight and recycles the rest, and a
//! `Repeater` that builds one for every row. The window over the model, the
//! pool of delegates gone out of sight, which row a pooled delegate may be
//! rebound to, rows leaving with an exit and coming back while they do, and
//! where each delegate goes -- all here. Building and rebinding a delegate is
//! the scripting layer's (`ViewHost`).

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use morf_scene::{ListModel, ModelId, NodeHandle, Scene, Value, VirtualList};

use crate::handler::Handler;

mod reconcile;

/// Every view, by the node it fills.
pub type Views = HashMap<NodeHandle, VirtualView>;

/// One row's delegate.
pub struct Delegate {
    pub node: NodeHandle,
    pub updater: Option<Handler>,
    /// The row it shows, as it was last given it: what a row put back while
    /// this one is still leaving is matched against.
    pub item: Value,
    /// The index it was last given (0-based): a row that moves keeps its
    /// delegate, which is rebound when this differs.
    pub index: usize,
}

/// A view over a list model.
pub struct VirtualView {
    pub model: Rc<RefCell<ListModel>>,
    pub view: VirtualList,
    pub delegate: Handler,
    pub active: HashMap<ModelId, Delegate>,
    pub reusable: HashMap<ModelId, Delegate>,
    pub reuse_order: VecDeque<ModelId>,
    pub reuse_limit: usize,
    pub pool_root: Option<NodeHandle>,
    /// Delegates of rows the model removed that are still playing their
    /// exit, in the parent, out of its flow.
    pub exiting: Vec<Delegate>,
    pub column_extent: f64,
    /// Whether the view places its delegates itself (a scrolling view) or
    /// leaves that to its own node's kind (a `Repeater`, which may be a
    /// `Row`, a `Column` or a `Grid`).
    pub positioned: bool,
    /// The row field that says how tall a row is, when rows differ.
    pub size_field: Option<String>,
    /// The row field that says which kind of delegate a row takes: a
    /// delegate is only ever reused for a row of its own kind.
    pub kind_field: Option<String>,
}

impl VirtualView {
    /// How long its content is, all its rows: what a scroll bar measures.
    pub fn content_extent(&self) -> f64 {
        self.view.content_extent(self.model.borrow().len())
    }
}

/// What a view needs of the scripting layer and the scene it lives in.
pub trait ViewHost {
    /// Builds the delegate for `item`, row `index`, from `delegate`.
    fn build(&mut self, delegate: &Handler, item: &Value, index: usize)
    -> Result<Delegate, String>;
    /// Rebinds a delegate to `item`, row `index`, through its `updater`, and
    /// settles what that changed.
    fn update(&mut self, updater: &Handler, item: &Value, index: usize) -> Result<(), String>;
    /// A new, bare node: the pool's.
    fn create(&mut self) -> NodeHandle;
    /// Removes `node` and everything under it.
    fn remove(&mut self, node: NodeHandle);
    /// Starts `node`'s exit, if it has one: whether it did.
    fn begin_exit(&mut self, node: NodeHandle) -> bool;
    /// Takes `node` back from its exit: whether it was leaving.
    fn cancel_exit(&mut self, node: NodeHandle) -> bool;
    /// Runs `f` on the scene.
    fn with_scene<R>(&mut self, f: impl FnOnce(&mut Scene) -> R) -> R;
}

/// Puts a positioned view's delegate for row `index` where it goes, the
/// view scrolled `offset` along.
pub fn position_view_child(
    scene: &mut Scene,
    node: NodeHandle,
    index: usize,
    view: &VirtualList,
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
pub fn row_number(item: &Value, field: &str) -> Option<f64> {
    match item {
        Value::Map(map) => match map.get(field)? {
            Value::Number(n) => Some(*n),
            _ => None,
        },
        _ => None,
    }
}

/// A row field read as text (a row's kind), if the row has it.
pub fn row_kind<'a>(item: &'a Value, field: Option<&str>) -> Option<&'a str> {
    match (item, field) {
        (Value::Map(map), Some(field)) => match map.get(field)? {
            Value::String(kind) => Some(kind),
            _ => None,
        },
        _ => None,
    }
}

/// Each row's extent, by its size field, for a list whose rows differ.
pub fn row_extents(model: &ListModel, field: &str, fallback: f64) -> Vec<f64> {
    (0..model.len())
        .map(|index| {
            model
                .get(index)
                .and_then(|(_, item)| row_number(item, field))
                .unwrap_or(fallback)
        })
        .collect()
}

#[cfg(test)]
mod tests;
