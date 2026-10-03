//! Tokens: one channel for colour, type, size and motion, inherited down
//! the tree and overridable on any subtree. A theme's tokens are a table;
//! a subtree's overrides are merged over the tokens it inherited, table
//! into table, so `{ color = { primary = ... } }` replaces one colour and
//! keeps the rest.

use std::collections::BTreeMap;
use std::sync::Arc;

use morf_lua::{IpcTable, IpcValue};

/// `overrides` merged over `parent`: nested tables merge, anything else
/// replaces.
pub fn merge_tokens(parent: &IpcValue, overrides: &IpcValue) -> IpcValue {
    match (parent, overrides) {
        (IpcValue::Table(base), IpcValue::Table(over)) => match (base.as_ref(), over.as_ref()) {
            (IpcTable::Map(base), IpcTable::Map(over)) => {
                let mut merged: BTreeMap<String, IpcValue> = base.clone();
                for (key, value) in over {
                    let next = match merged.get(key) {
                        Some(existing) => merge_tokens(existing, value),
                        None => value.clone(),
                    };
                    merged.insert(key.clone(), next);
                }
                IpcValue::Table(Arc::new(IpcTable::Map(merged)))
            }
            _ => overrides.clone(),
        },
        (_, IpcValue::Nil) => parent.clone(),
        _ => overrides.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(&str, IpcValue)]) -> IpcValue {
        IpcValue::Table(Arc::new(IpcTable::Map(
            entries
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
        )))
    }

    #[test]
    fn an_override_replaces_one_leaf_and_keeps_the_rest() {
        let parent = map(&[
            (
                "color",
                map(&[("primary", "#f00".into()), ("surface", "#000".into())]),
            ),
            ("radius", 8.0.into()),
        ]);
        let over = map(&[("color", map(&[("primary", "#0f0".into())]))]);
        let merged = merge_tokens(&parent, &over);
        let expected = map(&[
            (
                "color",
                map(&[("primary", "#0f0".into()), ("surface", "#000".into())]),
            ),
            ("radius", 8.0.into()),
        ]);
        assert_eq!(merged, expected);
    }
}
