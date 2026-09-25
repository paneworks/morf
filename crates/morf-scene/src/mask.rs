//! Alpha masks: what a node's `mask` property holds, and the node a mask may
//! be drawn from instead.
//!
//! A mask multiplies the alpha of everything a node and its subtree draw by
//! the alpha of something else at the same point — the way Qt's `MultiEffect`
//! takes a `maskSource`. That something is either a gradient across the
//! node's own box (`mask = { gradient = ... }`), which is plain data and lives
//! in the property, or a subtree of nodes (`mask = ui.Rect { ... }`), which
//! cannot: a property value never names a node. The subtree is then kept as a
//! child of the masked node, laid out in its box, and remembered in a side
//! table the way a field layer's `track` is (see [`Scene::set_mask`]).

use std::collections::BTreeMap;

use crate::gradient::Gradient;
use crate::{Color, NodeHandle, Scene, SceneError, Value};

/// A mask written as data: a gradient across the node's box whose alpha is
/// the mask's.
#[derive(Clone, Debug, PartialEq)]
pub struct MaskSpec {
    /// Only its alpha matters; its colours are kept as written.
    pub gradient: Gradient,
}

impl MaskSpec {
    /// Reads the `mask` property. Nil and an empty table mean no mask.
    ///
    /// A stop may be a bare number, which is the mask's alpha there: `{ 0, 1,
    /// 1, 0 }` fades in over the first third and out over the last.
    pub fn parse(value: &Value) -> Result<Option<Self>, String> {
        let entries = match value {
            Value::Nil => return Ok(None),
            Value::Map(entries) if entries.is_empty() => return Ok(None),
            Value::List(items) if items.is_empty() => return Ok(None),
            Value::Map(entries) => entries,
            _ => return Err("a mask is a node or a table with a gradient".to_owned()),
        };
        for key in entries.keys() {
            if key != "gradient" {
                return Err(format!("a mask has no `{key}`"));
            }
        }
        let gradient = entries
            .get("gradient")
            .ok_or("a mask table needs a gradient")?;
        let gradient = Gradient::parse(&alpha_stops(gradient.clone()))?
            .ok_or("a mask gradient needs stops")?;
        Ok(Some(Self { gradient }))
    }

    /// The value the property stores: what `parse` reads back unchanged, or an
    /// empty map for none.
    pub(crate) fn canonical(value: Value) -> Result<Value, String> {
        Ok(match Self::parse(&value)? {
            None => Value::Map(BTreeMap::new()),
            Some(spec) => {
                let mut entries = BTreeMap::new();
                entries.insert("gradient".to_owned(), spec.gradient.to_value());
                Value::Map(entries)
            }
        })
    }
}

/// White at the alpha a bare number gives, for every stop written as one.
fn alpha_stops(gradient: Value) -> Value {
    let Value::Map(mut entries) = gradient else {
        return gradient;
    };
    let alpha = |value: &Value| match value {
        Value::Number(alpha) if alpha.is_finite() => Some(Value::Color(Color {
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            alpha: alpha.clamp(0.0, 1.0) as f32,
        })),
        _ => None,
    };
    if let Some(Value::List(stops)) = entries.get_mut("stops") {
        for stop in stops.iter_mut() {
            match stop {
                Value::Number(_) => {
                    if let Some(color) = alpha(stop) {
                        *stop = color;
                    }
                }
                Value::List(pair) => {
                    if let Some(first) = pair.first_mut()
                        && let Some(color) = alpha(first)
                    {
                        *first = color;
                    }
                }
                Value::Map(entry) => {
                    if let Some(color) = entry.get("color").and_then(alpha) {
                        entry.insert("color".to_owned(), color);
                    }
                }
                _ => {}
            }
        }
    }
    Value::Map(entries)
}

impl Scene {
    /// Makes `mask`'s subtree the alpha mask of `node`, or takes it away.
    ///
    /// The mask becomes a child of `node`, so it is laid out in `node`'s box —
    /// filling it when it asks for no size and no anchors — and moves, scales
    /// and animates with it; but it is never painted or hit on its own, and a
    /// positioner does not give it a place. A mask it replaces is removed from
    /// the scene: it was made for this node and nothing else can hold it.
    pub fn set_mask(
        &mut self,
        node: NodeHandle,
        mask: Option<NodeHandle>,
    ) -> Result<(), SceneError> {
        let id = self.live(node)?;
        if let Some(mask) = mask {
            self.live(mask)?;
            if mask == node {
                return Err(self.mask_error(node, "a node cannot mask itself"));
            }
            // Its own ancestor would be a cycle, which `reparent` refuses too;
            // said here in the words a configuration used.
            let mut ancestor = Some(node);
            while let Some(current) = ancestor {
                if current == mask {
                    return Err(self.mask_error(node, "a mask cannot hold the node it masks"));
                }
                ancestor = self.parent(current)?;
            }
        }
        let previous = self.masks.get(&id).copied();
        if previous == mask {
            return Ok(());
        }
        if let Some(previous) = previous {
            self.masks.remove(&id);
            self.mask_owners.remove(&previous.id());
            if self.nodes.contains_key(previous.id()) {
                self.remove(previous)?;
            }
        }
        if let Some(mask) = mask {
            // A node that was already masking something else stops.
            if let Some(owner) = self.mask_owners.remove(&mask.id()) {
                self.masks.remove(&owner.id());
            }
            self.reparent(mask, Some(node))?;
            self.masks.insert(id, mask);
            self.mask_owners.insert(mask.id(), node);
        }
        self.bump_layout(id);
        Ok(())
    }

    /// The node whose subtree masks `node`, if it has one.
    pub fn mask(&self, node: NodeHandle) -> Option<NodeHandle> {
        self.masks
            .get(&node.id())
            .copied()
            .filter(|mask| self.nodes.contains_key(mask.id()))
    }

    /// Whether `node` is some other node's mask, and so laid out in its
    /// owner's box and never painted or hit on its own.
    pub fn is_mask(&self, node: NodeHandle) -> bool {
        self.mask_owners.contains_key(&node.id())
    }

    fn mask_error(&self, node: NodeHandle, message: &str) -> SceneError {
        SceneError::InvalidPropertyValue {
            element: self.element(node).map_or("node", |element| element.name()),
            property: "mask".to_owned(),
            message: message.to_owned(),
        }
    }
}
