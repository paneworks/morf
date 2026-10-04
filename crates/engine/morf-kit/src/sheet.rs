//! `Sheet`: a grid of cells the keyboard walks one cell at a time -- a
//! spreadsheet, a data grid with cells that edit in place, a step
//! sequencer, a seat map. A collection is rows; a sheet is cells.
//!
//! Settings: `rows`, `columns`, `row`, `column` (the current cell),
//! `editable` (true), `read_only` (column indices that do not edit),
//! `page_rows` (10: what Page Up and Page Down move), `wrap` (Tab past the
//! last column goes on to the next row; true), `toggle` (Space and a click
//! toggle the cell rather than edit it: a step sequencer).
//!
//! State: `row`, `column` (1-based), `anchor_row`, `anchor_column` (the
//! other corner of the selected range), `range` (`r0, c0, r1, c1`),
//! `editing`.
//!
//! Events: the base's; `"key"` (name, modifiers, text): the arrows move
//! (Shift extends the range, Ctrl goes to the edge), Home/End along the
//! row (Ctrl: the corners), Page Up/Down, Tab and Shift+Tab across, Return
//! down (Shift: up) -- committing an edit first --, F2 or a typed
//! character edits (the character starting the text), Escape cancels an
//! edit, Delete clears the range, Ctrl+A selects all, Ctrl+C, Ctrl+X and
//! Ctrl+V ask to copy, cut and paste; `"pressed"` (row, column,
//! modifiers: Shift extends, the press may start a range drag);
//! `"dragged"` (row, column); `"released"`; `"double_clicked"` (row,
//! column) edits; `"commit"` (text) and `"cancel"` end an edit.
//!
//! Signals: `current_changed` (row, column), `selection_changed` (r0, c0,
//! r1, c1), `edit_started` (row, column, text), `edited` (row, column,
//! text), `edit_canceled`, `toggled` (row, column), `cleared` (r0, c0, r1,
//! c1), `copy`, `cut` (r0, c0, r1, c1), `paste` (row, column),
//! `activated` (row, column).

use morf_value::{IpcTable, IpcValue};

use crate::control::ControlState;
use crate::value::{expect_boolean, expect_number, number, text};
use crate::{Archetype, Effects};

pub(crate) struct Sheet {
    pub(crate) base: ControlState,
    rows: usize,
    columns: usize,
    at: (usize, usize),
    anchor: (usize, usize),
    editable: bool,
    read_only: Vec<usize>,
    page_rows: usize,
    wrap: bool,
    toggle: bool,
    editing: bool,
    dragging: bool,
}

impl Sheet {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            rows: 1,
            columns: 1,
            at: (1, 1),
            anchor: (1, 1),
            editable: true,
            read_only: Vec::new(),
            page_rows: 10,
            wrap: true,
            toggle: false,
            editing: false,
            dragging: false,
        }
    }

    fn range(&self) -> (usize, usize, usize, usize) {
        (
            self.at.0.min(self.anchor.0),
            self.at.1.min(self.anchor.1),
            self.at.0.max(self.anchor.0),
            self.at.1.max(self.anchor.1),
        )
    }

    fn range_args(&self) -> Vec<IpcValue> {
        let (r0, c0, r1, c1) = self.range();
        vec![
            (r0 as i64).into(),
            (c0 as i64).into(),
            (r1 as i64).into(),
            (c1 as i64).into(),
        ]
    }

    fn fields(&self) -> Vec<(String, IpcValue)> {
        let (r0, c0, r1, c1) = self.range();
        vec![
            ("row".into(), (self.at.0 as i64).into()),
            ("column".into(), (self.at.1 as i64).into()),
            ("anchor_row".into(), (self.anchor.0 as i64).into()),
            ("anchor_column".into(), (self.anchor.1 as i64).into()),
            ("range".into(), format!("{r0},{c0},{r1},{c1}").into()),
            ("editing".into(), self.editing.into()),
        ]
    }

    fn changing(&mut self, change: impl FnOnce(&mut Self, &mut Effects)) -> Effects {
        let before = self.fields();
        let (at, range) = (self.at, self.range());
        let mut inner = Effects::default();
        change(self, &mut inner);
        let mut out = Effects::default();
        for (name, value) in self.fields() {
            if before.iter().find(|(n, _)| *n == name).map(|(_, v)| v) != Some(&value) {
                out.set(&name, value);
            }
        }
        if self.at != at {
            out.raise(
                "current_changed",
                vec![(self.at.0 as i64).into(), (self.at.1 as i64).into()],
            );
        }
        if self.range() != range {
            out.raise("selection_changed", self.range_args());
        }
        out.signals.extend(inner.signals);
        out.handled = inner.handled;
        out
    }

    fn clamp(&self, row: i64, column: i64) -> (usize, usize) {
        (
            row.clamp(1, self.rows.max(1) as i64) as usize,
            column.clamp(1, self.columns.max(1) as i64) as usize,
        )
    }

    fn go(&mut self, to: (usize, usize), extend: bool) {
        self.at = to;
        if !extend {
            self.anchor = to;
        }
    }

    fn can_edit(&self, column: usize) -> bool {
        self.editable && !self.toggle && !self.read_only.contains(&column)
    }

    fn start_edit(&mut self, with: &str, effects: &mut Effects) -> bool {
        if !self.can_edit(self.at.1) {
            return false;
        }
        self.editing = true;
        self.anchor = self.at;
        effects.raise(
            "edit_started",
            vec![
                (self.at.0 as i64).into(),
                (self.at.1 as i64).into(),
                with.into(),
            ],
        );
        true
    }

    fn key(&mut self, name: &str, modifiers: &str, typed: &str, effects: &mut Effects) -> bool {
        let ctrl = modifiers.contains("ctrl");
        let shift = modifiers.contains("shift");
        let (r, c) = (self.at.0 as i64, self.at.1 as i64);
        let (rows, columns) = (self.rows as i64, self.columns as i64);
        if self.editing {
            // An edit keeps every key but the ones that end it; the field
            // commits its own text with `commit`.
            return match name {
                "Escape" => {
                    self.editing = false;
                    effects.raise("edit_canceled", Vec::new());
                    true
                }
                _ => false,
            };
        }
        let right = if self.base.mirrored { -1 } else { 1 };
        let to = match name {
            "Left" | "Right" => {
                let d = if name == "Right" { right } else { -right };
                Some(if ctrl {
                    (r, if d > 0 { columns } else { 1 })
                } else {
                    (r, c + d)
                })
            }
            "Up" => Some(if ctrl { (1, c) } else { (r - 1, c) }),
            "Down" => Some(if ctrl { (rows, c) } else { (r + 1, c) }),
            "Home" => Some(if ctrl { (1, 1) } else { (r, 1) }),
            "End" => Some(if ctrl { (rows, columns) } else { (r, columns) }),
            "Page_Up" => Some((r - self.page_rows as i64, c)),
            "Page_Down" => Some((r + self.page_rows as i64, c)),
            "Tab" | "ISO_Left_Tab" if !ctrl => {
                let back = shift || name == "ISO_Left_Tab";
                let mut nr = r;
                let mut nc = c + if back { -1 } else { 1 };
                if nc > columns || nc < 1 {
                    if !self.wrap {
                        return false;
                    }
                    nc = if back { columns } else { 1 };
                    nr += if back { -1 } else { 1 };
                    if nr < 1 || nr > rows {
                        return false;
                    }
                }
                self.go(self.clamp(nr, nc), false);
                return true;
            }
            "Return" | "KP_Enter" => Some((r + if shift { -1 } else { 1 }, c)),
            _ => None,
        };
        if let Some((nr, nc)) = to {
            let extend = shift && !matches!(name, "Return" | "KP_Enter" | "Tab" | "ISO_Left_Tab");
            self.go(self.clamp(nr, nc), extend);
            return true;
        }
        let range = self.range_args();
        match name {
            "F2" => self.start_edit("", effects),
            "space" if self.toggle => {
                effects.raise("toggled", vec![r.into(), c.into()]);
                true
            }
            "Delete" | "BackSpace" => {
                effects.raise("cleared", range);
                true
            }
            "a" | "A" if ctrl => {
                self.anchor = (1, 1);
                self.at = (self.rows.max(1), self.columns.max(1));
                true
            }
            "c" | "C" if ctrl => {
                effects.raise("copy", range);
                true
            }
            "x" | "X" if ctrl => {
                effects.raise("cut", range);
                true
            }
            "v" | "V" if ctrl => {
                effects.raise("paste", vec![r.into(), c.into()]);
                true
            }
            _ => {
                // A character typed on a cell starts editing it with that
                // character -- as a spreadsheet does.
                let printable = !ctrl
                    && !modifiers.contains("alt")
                    && typed.chars().next().is_some_and(|ch| !ch.is_control());
                if printable {
                    self.start_edit(typed, effects)
                } else {
                    false
                }
            }
        }
    }
}

fn indices(value: &IpcValue) -> Vec<usize> {
    match value {
        IpcValue::Table(t) => match t.as_ref() {
            IpcTable::List(items) => items
                .iter()
                .filter_map(|v| number(Some(v)))
                .map(|n| n as usize)
                .collect(),
            IpcTable::Map(_) => Vec::new(),
        },
        _ => Vec::new(),
    }
}

impl Archetype for Sheet {
    fn name(&self) -> &'static str {
        "Sheet"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.fields());
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let cell = |s: &Self| -> Option<(usize, usize)> {
            Some(s.clamp(
                number(arguments.first())? as i64,
                number(arguments.get(1))? as i64,
            ))
        };
        Ok(match event {
            "key" if self.base.enabled => {
                let name = text(arguments.first()).unwrap_or("").to_owned();
                let modifiers = text(arguments.get(1)).unwrap_or("").to_owned();
                let typed = text(arguments.get(2)).unwrap_or("").to_owned();
                let mut used = false;
                let mut effects = self.changing(|s, e| used = s.key(&name, &modifiers, &typed, e));
                effects.handled = used;
                effects
            }
            "pressed" => {
                let mut effects = self.base.handle(event, arguments).unwrap_or_default();
                let Some(at) = cell(self) else {
                    return Ok(effects);
                };
                let extend = text(arguments.get(2)).is_some_and(|m| m.contains("shift"));
                effects.extend(self.changing(|s, e| {
                    if s.editing && at != s.at {
                        // A press elsewhere ends the edit; the field commits.
                        s.editing = false;
                        e.raise("edit_canceled", Vec::new());
                    }
                    s.go(at, extend);
                    s.dragging = true;
                    if s.toggle && !extend {
                        e.raise("toggled", vec![(at.0 as i64).into(), (at.1 as i64).into()]);
                    }
                }));
                effects
            }
            "dragged" => match cell(self) {
                Some(at) if self.dragging => self.changing(|s, _| s.at = at),
                _ => Effects::default(),
            },
            "released" | "canceled" => {
                let mut effects = self.base.handle(event, arguments).unwrap_or_default();
                self.dragging = false;
                effects.extend(Effects::default());
                effects
            }
            "double_clicked" => match cell(self) {
                Some(at) => self.changing(|s, e| {
                    s.go(at, false);
                    if !s.start_edit("", e) {
                        e.raise(
                            "activated",
                            vec![(at.0 as i64).into(), (at.1 as i64).into()],
                        );
                    }
                }),
                None => Effects::default(),
            },
            "commit" => {
                let typed = text(arguments.first()).unwrap_or("").to_owned();
                let down = !matches!(arguments.get(1), Some(IpcValue::Boolean(false)));
                self.changing(|s, e| {
                    if !s.editing {
                        return;
                    }
                    s.editing = false;
                    e.raise(
                        "edited",
                        vec![(s.at.0 as i64).into(), (s.at.1 as i64).into(), typed.into()],
                    );
                    if down {
                        let next = s.clamp(s.at.0 as i64 + 1, s.at.1 as i64);
                        s.go(next, false);
                    }
                })
            }
            "cancel" => self.changing(|s, e| {
                if s.editing {
                    s.editing = false;
                    e.raise("edit_canceled", Vec::new());
                }
            }),
            "clicked" | "key" | "long_pressed" | "drag_started" | "drag_finished" => {
                Effects::default()
            }
            _ => self.base.handle(event, arguments).unwrap_or_default(),
        })
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        let n = || expect_number(Some(value), field);
        Ok(match field {
            "rows" | "columns" => {
                let v = n()?.max(1.0) as usize;
                self.changing(|s, _| {
                    if field == "rows" {
                        s.rows = v
                    } else {
                        s.columns = v
                    }
                    s.at = s.clamp(s.at.0 as i64, s.at.1 as i64);
                    s.anchor = s.clamp(s.anchor.0 as i64, s.anchor.1 as i64);
                })
            }
            "row" | "column" => {
                let v = n()? as i64;
                self.changing(|s, _| {
                    let to = if field == "row" {
                        s.clamp(v, s.at.1 as i64)
                    } else {
                        s.clamp(s.at.0 as i64, v)
                    };
                    s.go(to, false);
                })
            }
            "editable" => {
                self.editable = expect_boolean(Some(value), field)?;
                Effects::default()
            }
            "read_only" => {
                self.read_only = indices(value);
                Effects::default()
            }
            "page_rows" => {
                self.page_rows = n()?.max(1.0) as usize;
                Effects::default()
            }
            "wrap" => {
                self.wrap = expect_boolean(Some(value), field)?;
                Effects::default()
            }
            "toggle" => {
                self.toggle = expect_boolean(Some(value), field)?;
                Effects::default()
            }
            _ => return Err(format!("Sheet has no setting `{field}`")),
        })
    }
}

#[cfg(test)]
mod tests;
