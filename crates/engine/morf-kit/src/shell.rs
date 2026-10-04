//! `Shell`: an application window's arrangement -- header bar, sidebar,
//! content, inspector, bottom bar -- and how it adapts as the window
//! narrows.
//!
//! Settings: `breakpoints` (widths, ascending, where the layout changes;
//! `{ 600, 900 }` by default), `layouts` (their names, one more than the
//! breakpoints: `{ "narrow", "medium", "wide" }`), `collapse_below` (the
//! width under which the sidebar becomes a drawer; the first breakpoint),
//! `inspector_below` (under which the inspector hides; the last),
//! `regions` (the names F6 walks: `{ "sidebar", "content" }`), `width`
//! (the window's, as it is laid out), `sidebar` (whether there is one).
//!
//! State: `width`, `layout`, `collapsed` (the sidebar is a drawer),
//! `sidebar_open` (shown: beside the content, or as an open drawer),
//! `inspector_shown`, `region` (the one F6 last moved to), `bottom_bar`
//! (narrow: the bottom bar stands in for the sidebar).
//!
//! Events: the base's, `"resize"` (width), `"toggle_sidebar"`, `"dismiss"`
//! (an open drawer shuts), `"key"` (F9 and Ctrl+B toggle the sidebar; F6
//! and Shift+F6 cycle the regions; Escape shuts an open drawer).
//! Signals: `breakpoint` (layout), `collapsed` (bool), `sidebar_toggled`
//! (open), `region` (name).

use morf_value::{IpcTable, IpcValue};

use crate::control::ControlState;
use crate::value::{expect_boolean, expect_number, text};
use crate::{Archetype, Effects};

pub(crate) struct Shell {
    pub(crate) base: ControlState,
    breakpoints: Vec<f64>,
    layouts: Vec<String>,
    collapse_below: Option<f64>,
    inspector_below: Option<f64>,
    regions: Vec<String>,
    region: usize,
    width: f64,
    has_sidebar: bool,
    /// Wide: whether the sidebar is shown beside the content. Collapsed:
    /// whether the drawer is open.
    shown_wide: bool,
    drawer_open: bool,
}

impl Shell {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            breakpoints: vec![600.0, 900.0],
            layouts: vec!["narrow".into(), "medium".into(), "wide".into()],
            collapse_below: None,
            inspector_below: None,
            regions: vec!["sidebar".into(), "content".into()],
            region: 1,
            width: 1200.0,
            has_sidebar: true,
            shown_wide: true,
            drawer_open: false,
        }
    }

    fn layout(&self) -> String {
        let index = self
            .breakpoints
            .iter()
            .filter(|b| self.width >= **b)
            .count();
        self.layouts
            .get(index)
            .cloned()
            .unwrap_or_else(|| format!("layout{index}"))
    }

    fn collapsed(&self) -> bool {
        let below = self
            .collapse_below
            .or(self.breakpoints.first().copied())
            .unwrap_or(0.0);
        self.has_sidebar && self.width < below
    }

    fn sidebar_open(&self) -> bool {
        self.has_sidebar
            && if self.collapsed() {
                self.drawer_open
            } else {
                self.shown_wide
            }
    }

    fn inspector_shown(&self) -> bool {
        let below = self
            .inspector_below
            .or(self.breakpoints.last().copied())
            .unwrap_or(0.0);
        self.width >= below
    }

    fn fields(&self) -> Vec<(String, IpcValue)> {
        vec![
            ("width".into(), self.width.into()),
            ("layout".into(), self.layout().into()),
            ("collapsed".into(), self.collapsed().into()),
            ("sidebar_open".into(), self.sidebar_open().into()),
            ("inspector_shown".into(), self.inspector_shown().into()),
            ("bottom_bar".into(), self.collapsed().into()),
            (
                "region".into(),
                self.regions
                    .get(self.region)
                    .cloned()
                    .unwrap_or_default()
                    .into(),
            ),
        ]
    }

    /// Runs `change` and says what it moved: every field that differs, and
    /// the signals for a new layout, collapse and sidebar.
    fn changing(&mut self, change: impl FnOnce(&mut Self)) -> Effects {
        let (layout, collapsed, open) = (self.layout(), self.collapsed(), self.sidebar_open());
        let before = self.fields();
        change(self);
        let mut effects = Effects::default();
        for (field, value) in self.fields() {
            if before.iter().find(|(f, _)| *f == field).map(|(_, v)| v) != Some(&value) {
                effects.set(&field, value);
            }
        }
        if self.layout() != layout {
            effects.raise("breakpoint", vec![self.layout().into()]);
        }
        if self.collapsed() != collapsed {
            // Collapsing shuts the drawer; widening shows the sidebar again.
            effects.raise("collapsed", vec![self.collapsed().into()]);
        }
        if self.sidebar_open() != open {
            effects.raise("sidebar_toggled", vec![self.sidebar_open().into()]);
        }
        effects
    }

    fn toggle(&mut self) -> Effects {
        self.changing(|s| {
            if s.collapsed() {
                s.drawer_open = !s.drawer_open;
            } else {
                s.shown_wide = !s.shown_wide;
            }
        })
    }

    fn cycle(&mut self, back: bool) -> Effects {
        let mut effects = Effects::default();
        if self.regions.is_empty() {
            return effects;
        }
        let n = self.regions.len();
        // A hidden sidebar is not a region to move to.
        for _ in 0..n {
            self.region = if back {
                (self.region + n - 1) % n
            } else {
                (self.region + 1) % n
            };
            if self.regions[self.region] != "sidebar" || self.sidebar_open() {
                break;
            }
        }
        let name = self.regions[self.region].clone();
        effects.set("region", name.clone());
        effects.raise("region", vec![name.into()]);
        effects
    }
}

fn numbers(value: &IpcValue) -> Vec<f64> {
    match value {
        IpcValue::Table(t) => match t.as_ref() {
            IpcTable::List(items) => items
                .iter()
                .filter_map(|v| match v {
                    IpcValue::Number(n) => Some(*n),
                    IpcValue::Integer(n) => Some(*n as f64),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn words(value: &IpcValue) -> Vec<String> {
    match value {
        IpcValue::Table(t) => match t.as_ref() {
            IpcTable::List(items) => items
                .iter()
                .filter_map(|v| text(Some(v)).map(str::to_owned))
                .collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

impl Archetype for Shell {
    fn name(&self) -> &'static str {
        "Shell"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.fields());
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        match event {
            "resize" => {
                let width = expect_number(arguments.first(), "width")?.max(0.0);
                Ok(self.changing(|s| {
                    let was = s.collapsed();
                    s.width = width;
                    if s.collapsed() != was {
                        s.drawer_open = false;
                    }
                }))
            }
            "toggle_sidebar" => Ok(self.toggle()),
            "dismiss" => Ok(self.changing(|s| s.drawer_open = false)),
            "key" if self.base.enabled => {
                let name = text(arguments.first()).unwrap_or("");
                let modifiers = text(arguments.get(1)).unwrap_or("");
                let ctrl = modifiers.contains("ctrl");
                let shift = modifiers.contains("shift");
                let mut effects = match name {
                    "F9" => self.toggle(),
                    "b" | "B" if ctrl => self.toggle(),
                    "F6" => self.cycle(shift),
                    "Escape" if self.collapsed() && self.drawer_open => {
                        self.changing(|s| s.drawer_open = false)
                    }
                    _ => return Ok(Effects::default()),
                };
                effects.handled = true;
                Ok(effects)
            }
            "clicked" | "key" => Ok(Effects::default()),
            _ => Ok(self.base.handle(event, arguments).unwrap_or_default()),
        }
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        let number = || expect_number(Some(value), field);
        Ok(match field {
            "breakpoints" => {
                let mut list = numbers(value);
                list.sort_by(f64::total_cmp);
                self.changing(|s| s.breakpoints = list)
            }
            "layouts" => {
                let list = words(value);
                self.changing(|s| s.layouts = list)
            }
            "collapse_below" => {
                let n = number()?;
                self.changing(|s| s.collapse_below = Some(n))
            }
            "inspector_below" => {
                let n = number()?;
                self.changing(|s| s.inspector_below = Some(n))
            }
            "regions" => {
                let list = words(value);
                self.regions = list;
                self.region = self.region.min(self.regions.len().saturating_sub(1));
                Effects::default()
            }
            "width" => {
                let n = number()?.max(0.0);
                self.changing(|s| s.width = n)
            }
            "sidebar" => {
                let on = expect_boolean(Some(value), field)?;
                self.changing(|s| s.has_sidebar = on)
            }
            _ => Effects::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(state: &[(String, IpcValue)], key: &str) -> IpcValue {
        state
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(IpcValue::Nil)
    }

    #[test]
    fn collapses_into_a_drawer_and_toggles_by_key() {
        let mut shell = Shell::new();
        assert_eq!(get(&shell.state(), "layout"), IpcValue::from("wide"));
        let effects = shell.handle("resize", &[IpcValue::Number(500.0)]).unwrap();
        assert!(effects.signals.iter().any(|s| s.0 == "collapsed"));
        assert_eq!(get(&shell.state(), "layout"), IpcValue::from("narrow"));
        assert_eq!(
            get(&shell.state(), "sidebar_open"),
            IpcValue::Boolean(false)
        );
        let effects = shell.handle("key", &["F9".into(), "".into()]).unwrap();
        assert!(effects.handled);
        assert_eq!(get(&shell.state(), "sidebar_open"), IpcValue::Boolean(true));
        shell.handle("key", &["Escape".into(), "".into()]).unwrap();
        assert_eq!(
            get(&shell.state(), "sidebar_open"),
            IpcValue::Boolean(false)
        );
        shell.handle("resize", &[IpcValue::Number(1000.0)]).unwrap();
        assert_eq!(get(&shell.state(), "sidebar_open"), IpcValue::Boolean(true));
        assert_eq!(
            get(&shell.state(), "inspector_shown"),
            IpcValue::Boolean(true)
        );
        let effects = shell.handle("key", &["F6".into(), "".into()]).unwrap();
        assert!(effects.signals.iter().any(|s| s.0 == "region"));
        assert_eq!(get(&shell.state(), "region"), IpcValue::from("sidebar"));
    }
}
