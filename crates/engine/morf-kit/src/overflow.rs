//! `Overflow`: items that share a line keep the ones that fit and put the
//! rest behind a "more" button -- a toolbar's actions, a tab strip, a
//! breadcrumb trail, a row of chips, a ribbon collapsing its groups.
//!
//! Settings: `widths` (each item's width, px, in order), `priorities`
//! (higher stays longer; equal ones go from the end -- or, `mode =
//! "middle"`, from the middle out, as a breadcrumb keeps its first and its
//! last), `pinned` (indices that never go), `available` (the room, px),
//! `more_width` (the "more" button's, 40), `gap` (px between items, 4),
//! `mode` (`"end"`, `"start"`, `"middle"`, `"priority"`).
//!
//! State: `shown` (indices that fit, encoded `,1,2,`), `hidden` (the rest,
//! in order), `overflowing`, `menu_open`.
//!
//! Events: `"resize"` (available), `"measure"` (index, width),
//! `"toggle_menu"`, `"close_menu"`. Signals: `changed` (shown, hidden as
//! lists).

use std::sync::Arc;

use morf_value::{IpcTable, IpcValue};

use crate::control::ControlState;
use crate::value::{expect_number, number, text};
use crate::{Archetype, Effects};

pub(crate) struct Overflow {
    pub(crate) base: ControlState,
    widths: Vec<f64>,
    priorities: Vec<f64>,
    pinned: Vec<usize>,
    available: f64,
    more_width: f64,
    gap: f64,
    mode: String,
    shown: Vec<usize>,
    hidden: Vec<usize>,
    menu_open: bool,
}

fn list(values: Vec<IpcValue>) -> IpcValue {
    IpcValue::Table(Arc::new(IpcTable::List(values)))
}

fn numbers(value: &IpcValue) -> Vec<f64> {
    match value {
        IpcValue::Table(t) => match t.as_ref() {
            IpcTable::List(items) => items.iter().filter_map(|v| number(Some(v))).collect(),
            IpcTable::Map(_) => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn encode(indices: &[usize]) -> String {
    if indices.is_empty() {
        return ",".into();
    }
    format!(
        ",{},",
        indices
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}

impl Overflow {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            widths: Vec::new(),
            priorities: Vec::new(),
            pinned: Vec::new(),
            available: f64::INFINITY,
            more_width: 40.0,
            gap: 4.0,
            mode: "end".into(),
            shown: Vec::new(),
            hidden: Vec::new(),
            menu_open: false,
        }
    }

    fn width_of(&self, set: &[usize]) -> f64 {
        let items: f64 = set
            .iter()
            .map(|i| self.widths.get(i - 1).copied().unwrap_or(0.0))
            .sum();
        items + self.gap * set.len().saturating_sub(1) as f64
    }

    /// The order items go in, first to go first: pinned ones never.
    fn going_order(&self) -> Vec<usize> {
        let n = self.widths.len();
        let mut order: Vec<usize> = (1..=n).filter(|i| !self.pinned.contains(i)).collect();
        let priority = |i: usize| self.priorities.get(i - 1).copied().unwrap_or(0.0);
        // Among equals, which goes first: the last from the end, the first
        // from the start, the nearest the middle from the middle.
        let rank = |i: usize| -> f64 {
            match self.mode.as_str() {
                "start" => i as f64,
                "middle" => (i as f64 - (n as f64 + 1.0) / 2.0).abs(),
                _ => -(i as f64),
            }
        };
        order.sort_by(|a, b| {
            priority(*a)
                .total_cmp(&priority(*b))
                .then(rank(*a).total_cmp(&rank(*b)))
        });
        order
    }

    fn fit(&mut self) {
        let n = self.widths.len();
        let mut shown: Vec<usize> = (1..=n).collect();
        let mut hidden = Vec::new();
        if self.width_of(&shown) > self.available {
            for going in self.going_order() {
                shown.retain(|i| *i != going);
                hidden.push(going);
                let need = self.width_of(&shown) + self.gap + self.more_width;
                if need <= self.available {
                    break;
                }
            }
        }
        hidden.sort_unstable();
        self.shown = shown;
        self.hidden = hidden;
        if self.hidden.is_empty() {
            self.menu_open = false;
        }
    }

    fn fields(&self) -> Vec<(String, IpcValue)> {
        vec![
            ("shown".into(), encode(&self.shown).into()),
            ("hidden".into(), encode(&self.hidden).into()),
            ("overflowing".into(), (!self.hidden.is_empty()).into()),
            ("menu_open".into(), self.menu_open.into()),
        ]
    }

    fn changing(&mut self, change: impl FnOnce(&mut Self)) -> Effects {
        let before = self.fields();
        let (shown, hidden) = (self.shown.clone(), self.hidden.clone());
        change(self);
        self.fit();
        let mut out = Effects::default();
        for (name, value) in self.fields() {
            if before.iter().find(|(n, _)| *n == name).map(|(_, v)| v) != Some(&value) {
                out.set(&name, value);
            }
        }
        if self.shown != shown || self.hidden != hidden {
            let as_list =
                |v: &[usize]| list(v.iter().map(|i| IpcValue::Integer(*i as i64)).collect());
            out.raise("changed", vec![as_list(&self.shown), as_list(&self.hidden)]);
        }
        out
    }
}

impl Archetype for Overflow {
    fn name(&self) -> &'static str {
        "Overflow"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.fields());
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let n = |i: usize| number(arguments.get(i)).unwrap_or(0.0);
        Ok(match event {
            "resize" => {
                let w = n(0).max(0.0);
                self.changing(|s| s.available = w)
            }
            "measure" => {
                let (i, w) = (n(0) as usize, n(1).max(0.0));
                self.changing(|s| {
                    if i >= 1 {
                        if s.widths.len() < i {
                            s.widths.resize(i, 0.0);
                        }
                        s.widths[i - 1] = w;
                    }
                })
            }
            "toggle_menu" => self.changing(|s| s.menu_open = !s.menu_open && !s.hidden.is_empty()),
            "close_menu" => self.changing(|s| s.menu_open = false),
            _ => self.base.handle(event, arguments).unwrap_or_default(),
        })
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        Ok(match field {
            "widths" => {
                let w = numbers(value);
                self.changing(|s| s.widths = w)
            }
            "priorities" => {
                let p = numbers(value);
                self.changing(|s| s.priorities = p)
            }
            "pinned" => {
                let p = numbers(value).into_iter().map(|v| v as usize).collect();
                self.changing(|s| s.pinned = p)
            }
            "available" => {
                let w = expect_number(Some(value), field)?.max(0.0);
                self.changing(|s| s.available = w)
            }
            "more_width" => {
                let w = expect_number(Some(value), field)?.max(0.0);
                self.changing(|s| s.more_width = w)
            }
            "gap" => {
                let g = expect_number(Some(value), field)?.max(0.0);
                self.changing(|s| s.gap = g)
            }
            "mode" => {
                let m = match text(Some(value)) {
                    Some(m @ ("end" | "start" | "middle" | "priority")) => m.to_owned(),
                    _ => return Err("mode is end, start, middle or priority".into()),
                };
                self.changing(|s| s.mode = m)
            }
            _ => return Err(format!("Overflow has no setting `{field}`")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(widths: &[f64]) -> Overflow {
        let mut o = Overflow::new();
        o.configure(
            "widths",
            &list(widths.iter().map(|w| (*w).into()).collect()),
        )
        .unwrap();
        o.configure("gap", &0.0.into()).unwrap();
        o
    }

    #[test]
    fn items_go_from_the_end_by_priority_and_a_breadcrumb_from_the_middle() {
        let mut o = bar(&[100.0, 100.0, 100.0, 100.0]);
        o.handle("resize", &[260.0.into()]).unwrap();
        // 2 x 100 + 40 for "more".
        assert_eq!(o.shown, vec![1, 2]);
        assert_eq!(o.hidden, vec![3, 4]);
        o.configure(
            "priorities",
            &list(vec![0.0.into(), 0.0.into(), 0.0.into(), 5.0.into()]),
        )
        .unwrap();
        assert_eq!(o.shown, vec![1, 4]);
        let mut crumbs = bar(&[60.0, 60.0, 60.0, 60.0, 60.0]);
        crumbs.configure("mode", &"middle".into()).unwrap();
        crumbs.handle("resize", &[200.0.into()]).unwrap();
        assert_eq!(crumbs.shown, vec![1, 5]);
        // Room again: everything back, the menu shut.
        crumbs.handle("toggle_menu", &[]).unwrap();
        assert!(crumbs.menu_open);
        crumbs.handle("resize", &[1000.0.into()]).unwrap();
        assert!(crumbs.hidden.is_empty() && !crumbs.menu_open);
    }
}
