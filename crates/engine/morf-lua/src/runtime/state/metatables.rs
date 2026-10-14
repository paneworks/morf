//! The metatables shared by every userdata of one kind: list models, scene
//! nodes and state proxies.

use luna::StashedTable;

#[derive(Default)]
pub(crate) struct Metatables {
    /// The list model's, kept so a list inside `morf.state` is the same
    /// kind of object as `morf.list_model` makes.
    model: Option<StashedTable>,
    /// The one every scene-node handle shares.
    ///
    /// Built on first use rather than at install time, because it needs the
    /// arena. Every node used to get its own — a fresh table and two fresh
    /// closures per node, neither of which captured anything node-specific, so
    /// a thousand-node tree allocated three thousand objects that were all the
    /// same. Every other userdata type in this crate already shares one.
    node: Option<StashedTable>,
    /// The one every state proxy shares, so a theme can be one.
    state: Option<StashedTable>,
}

impl Metatables {
    pub(crate) fn model(&self) -> Option<StashedTable> {
        self.model.clone()
    }

    pub(crate) fn set_model(&mut self, metatable: StashedTable) {
        self.model = Some(metatable);
    }

    pub(crate) fn node(&self) -> Option<StashedTable> {
        self.node.clone()
    }

    pub(crate) fn set_node(&mut self, metatable: StashedTable) {
        self.node = Some(metatable);
    }

    pub(crate) fn state(&self) -> Option<StashedTable> {
        self.state.clone()
    }

    pub(crate) fn set_state(&mut self, metatable: StashedTable) {
        self.state = Some(metatable);
    }
}
