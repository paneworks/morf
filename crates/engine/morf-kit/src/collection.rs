//! `Collection`: rows or cells from a model -- lists, grids, tables, trees
//! -- with `Selection`'s choosing and keys, and what a table and a tree add.
//! The rows themselves are built, recycled and placed by the engine's list
//! view; this keeps the behaviour.
//!
//! Settings: `Selection`'s, plus `layout` (`"list"`, `"grid"`, `"flow"`,
//! `"table"`, `"tree"`), `columns_spec` (a table's columns: `{ key,
//! width, sortable }`), `tree_rows` (a tree's shown rows in order:
//! `{ key, depth, children, expanded }`), `end_margin` (rows from the end
//! at which more is asked for, 5).
//!
//! State: `Selection`'s, `sort_column`, `sort_ascending`, `widths` (each
//! column's width, in order).
//!
//! Events: `Selection`'s, `"sort"` (column key), `"resize"` (column key,
//! width), `"toggle"` (row index: expand or collapse), and in a tree, Left
//! and Right collapse, expand and walk the levels. Signals: `Selection`'s,
//! `sort_changed` (column, ascending), `column_resized` (column, width),
//! `expanded_changed` (row key, expanded), `end_reached`.

use std::collections::BTreeMap;
use std::sync::Arc;

use morf_value::{IpcTable, IpcValue};

use crate::selection::Selection;
use crate::value::{expect_number, number, text};
use crate::{Archetype, Effects};

#[derive(Clone)]
struct Column {
    key: String,
    width: f64,
    sortable: bool,
}

#[derive(Clone)]
struct TreeRow {
    key: String,
    depth: i64,
    children: bool,
    expanded: bool,
}

pub(crate) struct Collection {
    selection: Selection,
    layout: String,
    columns: Vec<Column>,
    sort: Option<(String, bool)>,
    tree: Vec<TreeRow>,
    end_margin: i64,
    /// The row count at which `end_reached` was last said, so it is said
    /// once per batch of rows.
    asked_at: i64,
}

fn map_of(value: &IpcValue) -> Option<&BTreeMap<String, IpcValue>> {
    match value {
        IpcValue::Table(t) => match t.as_ref() {
            IpcTable::Map(map) => Some(map),
            _ => None,
        },
        _ => None,
    }
}

fn list_of(value: &IpcValue) -> Vec<IpcValue> {
    match value {
        IpcValue::Table(t) => match t.as_ref() {
            IpcTable::List(items) => items.clone(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

impl Collection {
    pub(crate) fn new() -> Self {
        Self {
            selection: Selection::new(),
            layout: "list".into(),
            columns: Vec::new(),
            sort: None,
            tree: Vec::new(),
            end_margin: 5,
            asked_at: -1,
        }
    }

    fn widths(&self) -> IpcValue {
        IpcValue::Table(Arc::new(IpcTable::List(
            self.columns.iter().map(|c| c.width.into()).collect(),
        )))
    }

    fn own_fields(&self) -> Vec<(String, IpcValue)> {
        vec![
            (
                "sort_column".into(),
                self.sort
                    .as_ref()
                    .map_or(IpcValue::from(""), |(k, _)| k.clone().into()),
            ),
            (
                "sort_ascending".into(),
                self.sort.as_ref().is_none_or(|(_, a)| *a).into(),
            ),
            ("widths".into(), self.widths()),
        ]
    }

    /// Whether the current row is near enough the end to ask for more.
    fn check_end(&mut self, effects: &mut Effects) {
        let count = self.selection.count();
        let current = self.selection.current();
        if count > 0 && current >= count - self.end_margin && self.asked_at != count {
            self.asked_at = count;
            effects.raise("end_reached", Vec::new());
        }
    }

    fn toggle(&mut self, index: i64, expand: Option<bool>, effects: &mut Effects) -> bool {
        let Some(row) = self
            .tree
            .get_mut((index - 1).max(0) as usize)
            .filter(|_| index >= 1)
        else {
            return false;
        };
        if !row.children {
            return false;
        }
        let next = expand.unwrap_or(!row.expanded);
        if next == row.expanded {
            return false;
        }
        row.expanded = next;
        effects.raise(
            "expanded_changed",
            vec![row.key.clone().into(), next.into()],
        );
        true
    }

    /// Left and Right in a tree: collapse, then up a level; expand, then
    /// down into the first child.
    fn tree_key(&mut self, name: &str, effects: &mut Effects) -> bool {
        let current = self.selection.current();
        let Some(row) = self
            .tree
            .get((current - 1).max(0) as usize)
            .filter(|_| current >= 1)
            .cloned()
        else {
            return false;
        };
        let (collapse, expand) = if self.selection.base.mirrored {
            ("Right", "Left")
        } else {
            ("Left", "Right")
        };
        if name == collapse {
            if row.children && row.expanded {
                return self.toggle(current, Some(false), effects);
            }
            // Up to the parent: the nearest row above that is shallower.
            let parent = (1..current).rev().find(|i| {
                self.tree
                    .get((*i - 1) as usize)
                    .is_some_and(|r| r.depth < row.depth)
            });
            if let Some(parent) = parent {
                return self.selection.go_to(parent, effects);
            }
            return false;
        }
        if name == expand {
            if row.children && !row.expanded {
                return self.toggle(current, Some(true), effects);
            }
            if row.children
                && self
                    .tree
                    .get(current as usize)
                    .is_some_and(|next| next.depth > row.depth)
            {
                return self.selection.go_to(current + 1, effects);
            }
        }
        false
    }
}

impl Archetype for Collection {
    fn name(&self) -> &'static str {
        "Collection"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.selection.state();
        fields.extend(self.own_fields());
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let mut effects = Effects::default();
        match event {
            "sort" => {
                let key = text(arguments.first()).unwrap_or("").to_owned();
                if !self.columns.iter().any(|c| c.key == key && c.sortable) {
                    return Ok(effects);
                }
                let ascending = match &self.sort {
                    Some((current, ascending)) if *current == key => !ascending,
                    _ => true,
                };
                self.sort = Some((key.clone(), ascending));
                for (f, v) in self.own_fields() {
                    effects.set(&f, v);
                }
                effects.raise("sort_changed", vec![key.into(), ascending.into()]);
            }
            "resize" => {
                let key = text(arguments.first()).unwrap_or("").to_owned();
                let width = number(arguments.get(1)).unwrap_or(0.0).max(24.0);
                if let Some(column) = self.columns.iter_mut().find(|c| c.key == key) {
                    column.width = width;
                    effects.set("widths", self.widths());
                    effects.raise("column_resized", vec![key.into(), width.into()]);
                }
            }
            "toggle" => {
                let index = number(arguments.first()).unwrap_or(0.0) as i64;
                self.toggle(index, None, &mut effects);
            }
            "key" if self.layout == "tree" => {
                let name = text(arguments.first()).unwrap_or("").to_owned();
                if self.tree_key(&name, &mut effects) {
                    effects.handled = true;
                } else {
                    effects = self.selection.handle(event, arguments)?;
                }
                self.check_end(&mut effects);
            }
            "key" | "item_pressed" => {
                effects = self.selection.handle(event, arguments)?;
                self.check_end(&mut effects);
            }
            _ => effects = self.selection.handle(event, arguments)?,
        }
        Ok(effects)
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        let mut effects = Effects::default();
        match field {
            "layout" => {
                let layout = text(Some(value)).unwrap_or("list");
                if !matches!(layout, "list" | "grid" | "flow" | "table" | "tree") {
                    return Err("layout is list, grid, flow, table or tree".into());
                }
                self.layout = layout.into();
                // A grid and a flow walk by rows of cells; the rest down.
                let orientation = if matches!(layout, "grid" | "flow") {
                    "grid"
                } else {
                    "vertical"
                };
                effects.extend(
                    self.selection
                        .configure("orientation", &orientation.into())?,
                );
            }
            "columns_spec" => {
                self.columns = list_of(value)
                    .iter()
                    .filter_map(map_of)
                    .map(|c| Column {
                        key: c
                            .get("key")
                            .and_then(|k| text(Some(k)))
                            .unwrap_or("")
                            .to_owned(),
                        width: c
                            .get("width")
                            .and_then(|w| number(Some(w)))
                            .unwrap_or(120.0),
                        sortable: matches!(c.get("sortable"), Some(IpcValue::Boolean(true))),
                    })
                    .collect();
                effects.set("widths", self.widths());
            }
            "tree_rows" => {
                self.tree = list_of(value)
                    .iter()
                    .filter_map(map_of)
                    .map(|r| TreeRow {
                        key: r
                            .get("key")
                            .and_then(|k| text(Some(k)))
                            .unwrap_or("")
                            .to_owned(),
                        depth: r.get("depth").and_then(|d| number(Some(d))).unwrap_or(0.0) as i64,
                        children: matches!(r.get("children"), Some(IpcValue::Boolean(true))),
                        expanded: matches!(r.get("expanded"), Some(IpcValue::Boolean(true))),
                    })
                    .collect();
            }
            "end_margin" => self.end_margin = expect_number(Some(value), field)?.max(0.0) as i64,
            _ => return self.selection.configure(field, value),
        }
        Ok(effects)
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

    fn list(items: Vec<IpcValue>) -> IpcValue {
        IpcValue::Table(Arc::new(IpcTable::List(items)))
    }

    #[test]
    fn a_table_sorts_by_a_column_and_flips_on_the_second_press() {
        let mut table = Collection::new();
        table
            .configure(
                "columns_spec",
                &list(vec![
                    map(&[("key", "name".into()), ("sortable", true.into())]),
                    map(&[("key", "size".into())]),
                ]),
            )
            .unwrap();
        let effects = table.handle("sort", &["name".into()]).unwrap();
        assert_eq!(
            effects.signals[0],
            ("sort_changed".into(), vec!["name".into(), true.into()])
        );
        let effects = table.handle("sort", &["name".into()]).unwrap();
        assert_eq!(effects.signals[0].1[1], false.into());
        assert!(
            table
                .handle("sort", &["size".into()])
                .unwrap()
                .signals
                .is_empty(),
            "not sortable"
        );
        let effects = table
            .handle("resize", &["size".into(), 10.0.into()])
            .unwrap();
        assert_eq!(
            effects.signals[0].1[1],
            24.0.into(),
            "a column keeps a least width"
        );
    }

    #[test]
    fn a_tree_expands_collapses_and_walks_its_levels() {
        let mut tree = Collection::new();
        tree.configure("layout", &"tree".into()).unwrap();
        tree.configure("count", &3.0.into()).unwrap();
        tree.configure(
            "tree_rows",
            &list(vec![
                map(&[
                    ("key", "a".into()),
                    ("depth", 0.0.into()),
                    ("children", true.into()),
                    ("expanded", false.into()),
                ]),
                map(&[("key", "b".into()), ("depth", 0.0.into())]),
                map(&[("key", "c".into()), ("depth", 0.0.into())]),
            ]),
        )
        .unwrap();
        tree.configure("current", &1.0.into()).unwrap();
        let effects = tree
            .handle("key", &["Right".into(), "".into(), "".into(), 0.0.into()])
            .unwrap();
        assert_eq!(
            effects.signals[0],
            ("expanded_changed".into(), vec!["a".into(), true.into()])
        );
        // The model shows a's child now.
        tree.configure("count", &4.0.into()).unwrap();
        tree.configure(
            "tree_rows",
            &list(vec![
                map(&[
                    ("key", "a".into()),
                    ("depth", 0.0.into()),
                    ("children", true.into()),
                    ("expanded", true.into()),
                ]),
                map(&[("key", "a1".into()), ("depth", 1.0.into())]),
                map(&[("key", "b".into()), ("depth", 0.0.into())]),
                map(&[("key", "c".into()), ("depth", 0.0.into())]),
            ]),
        )
        .unwrap();
        tree.handle("key", &["Right".into(), "".into(), "".into(), 0.0.into()])
            .unwrap();
        assert_eq!(tree.selection.current(), 2, "into the first child");
        tree.handle("key", &["Left".into(), "".into(), "".into(), 0.0.into()])
            .unwrap();
        assert_eq!(tree.selection.current(), 1, "up to the parent");
        let effects = tree
            .handle("key", &["Left".into(), "".into(), "".into(), 0.0.into()])
            .unwrap();
        assert_eq!(effects.signals[0].1[1], false.into(), "collapsed");
    }

    #[test]
    fn nearing_the_end_asks_for_more_once() {
        let mut list_view = Collection::new();
        list_view.configure("layout", &"list".into()).unwrap();
        list_view.configure("count", &10.0.into()).unwrap();
        list_view.configure("current", &4.0.into()).unwrap();
        let effects = list_view
            .handle("key", &["Down".into(), "".into(), "".into(), 0.0.into()])
            .unwrap();
        assert!(effects.signals.iter().any(|(n, _)| n == "end_reached"));
        let effects = list_view
            .handle("key", &["Down".into(), "".into(), "".into(), 0.0.into()])
            .unwrap();
        assert!(!effects.signals.iter().any(|(n, _)| n == "end_reached"));
    }
}
