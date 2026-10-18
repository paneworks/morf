//! The glass material's own properties: frost as an amount or `"auto"`, and
//! the `glass` table that sets the whole material in one write.

use crate::{animation::SceneError, types::*};

/// Each key `glass` takes, and the property it sets.
pub const GLASS_KEYS: [(&str, &str); 6] = [
    ("refraction", "glass_refraction"),
    ("frost", "glass_frost"),
    ("rim", "glass_rim"),
    ("tint", "backdrop_tint"),
    ("blur", "backdrop_blur"),
    ("saturation", "backdrop_saturation"),
];

/// How frosted a glass is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Frost {
    /// From 0, clear, to 1, all of it blurred.
    Amount(f64),
    /// Decided where it is drawn, by how busy what lies behind is.
    Auto,
}

impl Frost {
    /// `glass_frost` as it is kept: a number, clamped to 0..1, or `"auto"`.
    pub fn of(value: &Value) -> Self {
        match value {
            Value::String(name) if name == "auto" => Self::Auto,
            Value::Number(amount) if amount.is_finite() => Self::Amount(amount.clamp(0.0, 1.0)),
            _ => Self::Amount(1.0),
        }
    }
}

/// Checks a write to `glass_frost` or `glass`, refusing what neither takes.
pub(crate) fn canonical(property: &str, value: Value) -> Result<Value, String> {
    if property == "glass_frost" {
        return match &value {
            Value::Number(amount) if amount.is_finite() => Ok(value),
            Value::String(name) if name == "auto" => Ok(value),
            _ => Err("a number from 0 (clear) to 1 (frosted), or \"auto\"".to_owned()),
        };
    }
    let keys = || {
        GLASS_KEYS
            .iter()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>()
            .join(", ")
    };
    match &value {
        Value::Nil => Ok(value),
        Value::Map(map) => match map
            .keys()
            .find(|key| !GLASS_KEYS.iter().any(|(name, _)| name == key))
        {
            Some(unknown) => Err(format!("`{unknown}` is not part of glass: use {}", keys())),
            None => Ok(value),
        },
        _ => Err(format!("a table of {}", keys())),
    }
}

impl Scene {
    /// Whether any of `node`'s glass properties has ever moved: false is a
    /// promise they all hold their defaults, so paint need not read them.
    pub fn may_have_glass(&self, node: NodeHandle) -> bool {
        !self.glass_nodes.is_empty() && self.glass_nodes.contains(&node.0)
    }

    /// Assigns each property the node's `glass` table names, each through
    /// [`Scene::assign`], so its behaviors and springs carry the change.
    pub(crate) fn expand_glass(&mut self, node: NodeHandle) -> Result<(), SceneError> {
        let Value::Map(table) = self.current(node, "glass")?.clone() else {
            return Ok(());
        };
        for (key, property) in GLASS_KEYS {
            if let Some(value) = table.get(key) {
                self.assign(node, property, value.clone())?;
            }
        }
        Ok(())
    }
}
