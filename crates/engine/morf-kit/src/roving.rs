//! `Roving`: one Tab stop for a group of controls the arrows move between
//! -- a toolbar, a menu bar, a button group, a row of chips. Its members
//! are controls of their own (presses, toggles, menu buttons); the group
//! only says which of them has focus. Tab enters at the one last focused
//! and leaves past the group.
//!
//! Settings: `count`, `current` (1-based), `orientation` (`"horizontal"`,
//! `"vertical"`, `"grid"`), `columns` (a grid's), `wrap` (true),
//! `disabled` (indices the arrows pass over), `menubar` (members open
//! menus: Down, Return and Space open the current one, and while one is
//! open Left and Right open the next).
//!
//! State: `current`, `open` (a menu bar's member whose menu is open, or 0).
//!
//! Events: the base's; `"key"` (name, modifiers); `"focus_in"` (index: a
//! member took focus, by pointer or Tab); `"menu_closed"`. Signals:
//! `current_changed` (index) -- the glue focuses that member --, `open`
//! (index), `close`.

use morf_value::{IpcTable, IpcValue};

use crate::control::ControlState;
use crate::value::{expect_boolean, expect_number, number, text};
use crate::{Archetype, Effects};

pub(crate) struct Roving {
    pub(crate) base: ControlState,
    count: usize,
    current: usize,
    vertical: bool,
    grid: bool,
    columns: usize,
    wrap: bool,
    disabled: Vec<usize>,
    menubar: bool,
    open: usize,
}

impl Roving {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            count: 0,
            current: 1,
            vertical: false,
            grid: false,
            columns: 1,
            wrap: true,
            disabled: Vec::new(),
            menubar: false,
            open: 0,
        }
    }

    fn fields(&self) -> Vec<(String, IpcValue)> {
        vec![("current".into(), (self.current as i64).into()), ("open".into(), (self.open as i64).into())]
    }

    /// The next member `step` away that is not disabled, if any.
    fn step(&self, from: usize, step: i64) -> Option<usize> {
        let n = self.count as i64;
        if n == 0 {
            return None;
        }
        let mut at = from as i64;
        for _ in 0..n {
            at += step;
            if at < 1 || at > n {
                if !self.wrap {
                    return None;
                }
                at = if at < 1 { at + n } else { at - n };
            }
            if !self.disabled.contains(&(at as usize)) {
                return Some(at as usize);
            }
        }
        None
    }

    fn edge(&self, last: bool) -> Option<usize> {
        let mut order: Vec<usize> = (1..=self.count).collect();
        if last {
            order.reverse();
        }
        order.into_iter().find(|i| !self.disabled.contains(i))
    }

    fn move_to(&mut self, to: usize, effects: &mut Effects) {
        if to != self.current {
            self.current = to;
            effects.set("current", to as i64);
        }
        // (Said even when it stays: the glue puts focus back on it.)
        effects.raise("current_changed", vec![(to as i64).into()]);
        if self.open > 0 {
            self.open = to;
            effects.set("open", to as i64);
            effects.raise("open", vec![(to as i64).into()]);
        }
    }
}

impl Archetype for Roving {
    fn name(&self) -> &'static str {
        "Roving"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.fields());
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let mut effects = Effects::default();
        match event {
            "key" if self.base.enabled => {
                let name = text(arguments.first()).unwrap_or("");
                let right = if self.base.mirrored { -1 } else { 1 };
                let cols = if self.grid { self.columns.max(1) as i64 } else { 1 };
                let target = match name {
                    "Left" if !self.vertical || self.grid => self.step(self.current, -right),
                    "Right" if !self.vertical || self.grid => self.step(self.current, right),
                    "Up" if self.vertical || self.grid => self.step(self.current, -cols),
                    "Down" if self.vertical || self.grid => self.step(self.current, cols),
                    "Home" => self.edge(false),
                    "End" => self.edge(true),
                    _ => None,
                };
                if let Some(to) = target {
                    self.move_to(to, &mut effects);
                    effects.handled = true;
                    return Ok(effects);
                }
                if self.menubar {
                    match name {
                        "Down" | "Return" | "KP_Enter" | "space" if self.open == 0 => {
                            self.open = self.current;
                            effects.set("open", self.open as i64);
                            effects.raise("open", vec![(self.open as i64).into()]);
                            effects.handled = true;
                        }
                        "Escape" if self.open > 0 => {
                            self.open = 0;
                            effects.set("open", 0i64);
                            effects.raise("close", Vec::new());
                            effects.handled = true;
                        }
                        _ => {}
                    }
                }
            }
            "focus_in" => {
                if let Some(i) = number(arguments.first()) {
                    let i = (i as usize).clamp(1, self.count.max(1));
                    if i != self.current {
                        self.current = i;
                        effects.set("current", i as i64);
                    }
                }
            }
            "menu_closed" => {
                if self.open > 0 {
                    self.open = 0;
                    effects.set("open", 0i64);
                }
            }
            "clicked" | "key" | "pressed" | "released" => {
                return Ok(self.base.handle(event, arguments).unwrap_or_default());
            }
            _ => return Ok(self.base.handle(event, arguments).unwrap_or_default()),
        }
        Ok(effects)
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        let mut effects = Effects::default();
        match field {
            "count" => {
                self.count = expect_number(Some(value), field)?.max(0.0) as usize;
                self.current = self.current.clamp(1, self.count.max(1));
                effects.set("current", self.current as i64);
            }
            "current" => {
                self.current = (expect_number(Some(value), field)? as usize).clamp(1, self.count.max(1));
                effects.set("current", self.current as i64);
            }
            "orientation" => {
                let o = text(Some(value)).unwrap_or("horizontal");
                self.vertical = o == "vertical";
                self.grid = o == "grid";
            }
            "columns" => self.columns = expect_number(Some(value), field)?.max(1.0) as usize,
            "wrap" => self.wrap = expect_boolean(Some(value), field)?,
            "menubar" => self.menubar = expect_boolean(Some(value), field)?,
            "disabled" => {
                self.disabled = match value {
                    IpcValue::Table(t) => match t.as_ref() {
                        IpcTable::List(items) => items.iter().filter_map(|v| number(Some(v))).map(|n| n as usize).collect(),
                        IpcTable::Map(_) => Vec::new(),
                    },
                    _ => Vec::new(),
                }
            }
            _ => return Err(format!("Roving has no setting `{field}`")),
        }
        Ok(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_skip_disabled_members_and_a_menubar_opens_and_follows() {
        let mut r = Roving::new();
        r.configure("count", &4.0.into()).unwrap();
        r.configure("disabled", &IpcValue::Table(std::sync::Arc::new(IpcTable::List(vec![2.0.into()])))).unwrap();
        let e = r.handle("key", &["Right".into(), "".into()]).unwrap();
        assert!(e.handled);
        assert_eq!(r.current, 3);
        r.handle("key", &["End".into(), "".into()]).unwrap();
        assert_eq!(r.current, 4);
        r.handle("key", &["Right".into(), "".into()]).unwrap();
        assert_eq!(r.current, 1);
        r.configure("menubar", &true.into()).unwrap();
        let e = r.handle("key", &["Down".into(), "".into()]).unwrap();
        assert!(e.signals.iter().any(|(n, a)| n == "open" && a[0] == 1i64.into()));
        let e = r.handle("key", &["Right".into(), "".into()]).unwrap();
        assert!(e.signals.iter().any(|(n, a)| n == "open" && a[0] == 3i64.into()));
        let e = r.handle("key", &["Escape".into(), "".into()]).unwrap();
        assert!(e.signals.iter().any(|(n, _)| n == "close"));
        // Up and Down mean nothing to a horizontal toolbar.
        r.configure("menubar", &false.into()).unwrap();
        assert!(!r.handle("key", &["Up".into(), "".into()]).unwrap().handled);
    }
}
