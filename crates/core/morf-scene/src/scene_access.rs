use crate::{animation::*, hashing::*, types::*};

impl Scene {
    /// Reads the value currently used by layout or paint.
    pub fn current(&self, node: NodeHandle, property: &str) -> Result<&Value, SceneError> {
        let slot = self.property(node, property)?;
        Ok(self.properties.read(slot.current)?)
    }

    /// Reads the settled value most recently produced by a binding or assignment.
    pub fn target(&self, node: NodeHandle, property: &str) -> Result<&Value, SceneError> {
        let slot = self.property(node, property)?;
        Ok(self.properties.read(slot.target)?)
    }

    /// Reads a numeric current property.
    pub fn number(&self, node: NodeHandle, property: &str) -> Result<f64, SceneError> {
        match self.current(node, property)? {
            Value::Number(value) => Ok(*value),
            _ => Err(SceneError::Reactive(format!(
                "property `{property}` is not numeric"
            ))),
        }
    }

    /// Reads a string current property.
    pub fn string_value(&self, node: NodeHandle, property: &str) -> Result<&str, SceneError> {
        match self.current(node, property)? {
            Value::String(value) => Ok(value),
            _ => Err(SceneError::Reactive(format!(
                "property `{property}` is not a string"
            ))),
        }
    }

    /// Reads a boolean current property.
    pub fn bool_value(&self, node: NodeHandle, property: &str) -> Result<bool, SceneError> {
        match self.current(node, property)? {
            Value::Bool(value) => Ok(*value),
            _ => Err(SceneError::Reactive(format!(
                "property `{property}` is not boolean"
            ))),
        }
    }

    /// Reads a color current property.
    pub fn color_value(&self, node: NodeHandle, property: &str) -> Result<Color, SceneError> {
        match self.current(node, property)? {
            Value::Color(value) => Ok(*value),
            _ => Err(SceneError::Reactive(format!(
                "property `{property}` is not a color"
            ))),
        }
    }

    pub(crate) fn live(&self, node: NodeHandle) -> Result<NodeId, SceneError> {
        self.nodes
            .contains_key(node.0)
            .then_some(node.0)
            .ok_or(SceneError::StaleNode)
    }

    pub(crate) fn property(
        &self,
        node: NodeHandle,
        property: &str,
    ) -> Result<PropertySlot, SceneError> {
        // One arena lookup, not three: `live` used to probe for the node and
        // then index it twice more. Every property read of every node in every
        // frame goes through here, so the difference is worth the directness.
        let node = self.node_ref(node)?;
        Self::slot_of(&self.defaults, node, property).map(|(_, slot)| slot)
    }

    /// A live node's slot for `property`, and its name as the schema spells
    /// it: the node's own, or the element's default when nothing set it.
    /// For reading only: [`Scene::owned`] gives the slot a write needs.
    pub(crate) fn lookup(
        &self,
        id: NodeId,
        property: &str,
    ) -> Result<(&'static str, PropertySlot), SceneError> {
        Self::slot_of(&self.defaults, &self.nodes[id], property)
    }

    /// A live node's own slot for `property`, made from the default when
    /// nothing set it yet: what an assignment or a motion writes into.
    pub(crate) fn owned(
        &mut self,
        id: NodeId,
        property: &str,
    ) -> Result<(&'static str, PropertySlot), SceneError> {
        if let Some((name, slot)) = self.nodes[id].properties.get_key_value(property) {
            return Ok((*name, *slot));
        }
        let (name, shared) = self.lookup(id, property)?;
        let value = self.properties.read(shared.current)?.clone();
        let slot = PropertySlot {
            current: self.properties.signal(name, value.clone()),
            target: self.properties.signal(name, value),
            kind: shared.kind,
        };
        self.nodes[id].properties.insert(name, slot);
        Ok((name, slot))
    }

    fn slot_of(
        defaults: &FastMap<Element, FastMap<&'static str, PropertySlot>>,
        node: &Node,
        property: &str,
    ) -> Result<(&'static str, PropertySlot), SceneError> {
        node.properties
            .get_key_value(property)
            .or_else(|| defaults.get(&node.element)?.get_key_value(property))
            .map(|(name, slot)| (*name, *slot))
            .ok_or_else(|| SceneError::UnknownProperty {
                element: node.element.name(),
                property: property.to_owned(),
            })
    }

    /// Makes `element`'s default slots, the first time a node of it is made.
    pub(crate) fn ensure_defaults(&mut self, element: Element) {
        if self.defaults.contains_key(&element) {
            return;
        }
        let slots = crate::schema::schema(element)
            .into_iter()
            .map(|spec| {
                let shared = self.properties.signal(spec.name, spec.default);
                let slot = PropertySlot {
                    current: shared,
                    target: shared,
                    kind: spec.kind,
                };
                (spec.name, slot)
            })
            .collect();
        self.defaults.insert(element, slots);
    }

    /// Borrows a live node, or reports that its handle has gone stale.
    pub(crate) fn node_ref(&self, node: NodeHandle) -> Result<&Node, SceneError> {
        self.nodes.get(node.id()).ok_or(SceneError::StaleNode)
    }
}

impl Scene {
    /// Attaches a compiled shader to a node.
    ///
    /// The program is named by the hash of its generated WGSL, which is what
    /// the renderer registered it under: the scene never holds shader source
    /// and never compiles anything.
    pub fn attach_shader(&mut self, node: NodeHandle, shader: NodeShader) {
        self.shaders.insert(node.0, shader);
        self.bump_paint(node.0);
    }

    /// Removes a node's shader, if it had one.
    pub fn detach_shader(&mut self, node: NodeHandle) {
        if self.shaders.remove(&node.0).is_some() {
            self.bump_paint(node.0);
        }
    }

    /// Hangs a picture of a terminal's screen on its node, for the renderer.
    pub fn set_terminal_screen(
        &mut self,
        node: NodeHandle,
        screen: std::sync::Arc<crate::TerminalScreen>,
    ) {
        if self.nodes.contains_key(node.0) {
            self.terminal_screens.insert(node.0, screen);
            self.bump_paint(node.0);
        }
    }

    /// What a `Terminal` node's screen last showed.
    pub fn terminal_screen(
        &self,
        node: NodeHandle,
    ) -> Option<&std::sync::Arc<crate::TerminalScreen>> {
        self.terminal_screens.get(&node.0)
    }

    /// Makes a field layer follow another node's rendered geometry, or stop.
    ///
    /// The layer's rectangle is then wherever `target` is drawn this frame —
    /// its layout box through every transform above it and its own, a stretch
    /// included — resolved by the renderer as it paints, so a layer follows
    /// a node that a behavior or a spring is moving without the configuration
    /// doing anything per frame.
    pub fn set_track(
        &mut self,
        node: NodeHandle,
        target: Option<NodeHandle>,
    ) -> Result<(), SceneError> {
        let id = self.live(node)?;
        match target {
            Some(target) => {
                self.live(target)?;
                if target == node {
                    return Err(SceneError::InvalidPropertyValue {
                        element: self.element(node)?.name(),
                        property: "track".to_owned(),
                        message: "a layer cannot follow itself".to_owned(),
                    });
                }
                self.tracks.insert(id, target);
            }
            None => {
                self.tracks.remove(&id);
            }
        }
        self.bump_paint(id);
        Ok(())
    }

    /// The node a field layer follows, if it follows one that still exists.
    pub fn track(&self, node: NodeHandle) -> Option<NodeHandle> {
        self.tracks
            .get(&node.0)
            .copied()
            .filter(|target| self.nodes.contains_key(target.0))
    }

    /// The shader attached to a node.
    pub fn node_shader(&self, node: NodeHandle) -> Option<&NodeShader> {
        self.shaders.get(&node.0)
    }

    /// Fills one of an attached shader's data blocks.
    ///
    /// The values are copied, truncated or zero-padded to the length the
    /// shader declared — a configuration handing over the wrong number of them
    /// is a mistake worth surviving, because the alternative is a frame that
    /// reads past the end of a buffer.
    pub fn set_shader_data(&mut self, node: NodeHandle, index: usize, values: &[f32]) {
        if let Some(shader) = self.shaders.get_mut(&node.0)
            && let Some(block) = shader.data.get_mut(index)
        {
            let length = block.len();
            block.clear();
            block.extend(values.iter().take(length).copied());
            block.resize(length, 0.0);
            self.bump_paint(node.0);
        }
    }

    /// Sets one parameter of an attached shader.
    ///
    /// Out-of-range indices are ignored rather than panicking: the index comes
    /// from a configuration, and a shader can be swapped for one with fewer
    /// parameters between the write and the read.
    pub fn set_shader_param(&mut self, node: NodeHandle, index: usize, value: f32) {
        if let Some(shader) = self.shaders.get_mut(&node.0)
            && let Some(slot) = shader.params.get_mut(index)
        {
            *slot = value;
            self.bump_paint(node.0);
        }
    }
}
