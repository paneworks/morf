//! `Navigation`: pages that change with transitions -- a navigation view's
//! push and pop, a view stack, a carousel, a wizard.
//!
//! Settings: `mode` (`"stack"`, `"switcher"`, `"carousel"`, `"wizard"`),
//! `pages` (the page names, for a switcher, carousel or wizard), `current`
//! (a page name: the root of a stack, the shown one otherwise), `wrap`.
//!
//! State: `current`, `depth` (how deep a stack is), `can_go_back`,
//! `can_go_forward`, `direction` (1 forward, -1 back: which way the last
//! change went, for a skin's transition).
//!
//! Events: the base's, `"push"` (page), `"pop"`, `"go"` (page), `"next"`,
//! `"previous"`, `"key"` (Alt+Left and the back key pop; Ctrl+Tab and
//! Ctrl+Shift+Tab cycle a switcher; the arrows walk a carousel). Signals:
//! `pushed` (page), `popped` (page), `current_changed` (page, direction).

use morf_value::{IpcTable, IpcValue};

use crate::control::ControlState;
use crate::value::{expect_boolean, text};
use crate::{Archetype, Effects};

pub(crate) struct Navigation {
    pub(crate) base: ControlState,
    mode: String,
    pages: Vec<String>,
    /// A stack's pages, root first; the shown page of the others.
    history: Vec<String>,
    wrap: bool,
    direction: i64,
}

impl Navigation {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            mode: "stack".into(),
            pages: Vec::new(),
            history: Vec::new(),
            wrap: false,
            direction: 1,
        }
    }

    fn current(&self) -> String {
        self.history.last().cloned().unwrap_or_default()
    }

    fn fields_into(&self, effects: &mut Effects) {
        for (field, value) in self.nav_fields() {
            effects.set(&field, value);
        }
    }

    fn nav_fields(&self) -> Vec<(String, IpcValue)> {
        let index = self.pages.iter().position(|p| *p == self.current());
        let (back, forward) = match self.mode.as_str() {
            "stack" => (self.history.len() > 1, false),
            _ => (
                index.is_some_and(|i| i > 0) || (self.wrap && self.pages.len() > 1),
                index.is_some_and(|i| i + 1 < self.pages.len()) || (self.wrap && self.pages.len() > 1),
            ),
        };
        vec![
            ("current".into(), self.current().into()),
            ("depth".into(), (self.history.len() as i64).into()),
            ("can_go_back".into(), back.into()),
            ("can_go_forward".into(), forward.into()),
            ("direction".into(), self.direction.into()),
        ]
    }

    fn show(&mut self, page: String, direction: i64, effects: &mut Effects) {
        if page == self.current() {
            return;
        }
        self.direction = direction;
        if self.mode == "stack" {
            self.history.push(page.clone());
        } else {
            self.history = vec![page.clone()];
        }
        self.fields_into(effects);
        effects.raise("current_changed", vec![page.into(), direction.into()]);
    }

    fn step(&mut self, by: i64, effects: &mut Effects) -> bool {
        let count = self.pages.len() as i64;
        let Some(at) = self.pages.iter().position(|p| *p == self.current()) else {
            return false;
        };
        let mut next = at as i64 + by;
        if next < 0 || next >= count {
            if !self.wrap {
                return false;
            }
            next = next.rem_euclid(count);
        }
        let page = self.pages[next as usize].clone();
        self.show(page, by.signum(), effects);
        true
    }

    fn pop(&mut self, effects: &mut Effects) -> bool {
        if self.mode != "stack" {
            return self.step(-1, effects);
        }
        if self.history.len() <= 1 {
            return false;
        }
        let gone = self.history.pop().unwrap_or_default();
        self.direction = -1;
        self.fields_into(effects);
        effects.raise("popped", vec![gone.into()]);
        effects.raise("current_changed", vec![self.current().into(), IpcValue::Integer(-1)]);
        true
    }
}

impl Archetype for Navigation {
    fn name(&self) -> &'static str {
        "Navigation"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.nav_fields());
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let mut effects = Effects::default();
        match event {
            "push" => {
                let page = text(arguments.first()).ok_or("push wants a page name")?.to_owned();
                if self.mode == "stack" {
                    if page != self.current() {
                        self.direction = 1;
                        self.history.push(page.clone());
                        self.fields_into(&mut effects);
                        effects.raise("pushed", vec![page.clone().into()]);
                        effects.raise("current_changed", vec![page.into(), IpcValue::Integer(1)]);
                    }
                } else {
                    let forward = self.pages.iter().position(|p| *p == page)
                        >= self.pages.iter().position(|p| *p == self.current());
                    self.show(page, if forward { 1 } else { -1 }, &mut effects);
                }
            }
            "go" => {
                let page = text(arguments.first()).ok_or("go wants a page name")?.to_owned();
                if self.mode == "stack" {
                    // Back to a page already in the stack, or a new root.
                    if let Some(at) = self.history.iter().position(|p| *p == page) {
                        while self.history.len() > at + 1 {
                            self.pop(&mut effects);
                        }
                    } else {
                        self.history = vec![page.clone()];
                        self.direction = 1;
                        self.fields_into(&mut effects);
                        effects.raise("current_changed", vec![page.into(), IpcValue::Integer(1)]);
                    }
                } else {
                    let forward = self.pages.iter().position(|p| *p == page)
                        >= self.pages.iter().position(|p| *p == self.current());
                    self.show(page, if forward { 1 } else { -1 }, &mut effects);
                }
            }
            "pop" => {
                self.pop(&mut effects);
            }
            "next" => {
                self.step(1, &mut effects);
            }
            "previous" => {
                self.step(-1, &mut effects);
            }
            "key" if self.base.enabled => {
                let name = text(arguments.first()).unwrap_or("");
                let modifiers = text(arguments.get(1)).unwrap_or("");
                let alt = modifiers.contains("alt");
                let ctrl = modifiers.contains("ctrl");
                let back_key = if self.base.mirrored { "Right" } else { "Left" };
                let handled = match name {
                    "XF86Back" => self.pop(&mut effects),
                    n if n == back_key && alt => self.pop(&mut effects),
                    "Tab" | "ISO_Left_Tab" if ctrl && self.mode == "switcher" => {
                        let back = name == "ISO_Left_Tab" || modifiers.contains("shift");
                        self.step(if back { -1 } else { 1 }, &mut effects)
                    }
                    "Left" | "Right" if self.mode == "carousel" && !alt => {
                        let forward = (name == "Right") != self.base.mirrored;
                        self.step(if forward { 1 } else { -1 }, &mut effects)
                    }
                    _ => false,
                };
                effects.handled = handled;
            }
            "clicked" | "key" => {}
            _ => return self.base.handle(event, arguments).ok_or_else(|| format!("Navigation has no event `{event}`")),
        }
        Ok(effects)
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        let mut effects = Effects::default();
        match field {
            "mode" => {
                let mode = text(Some(value)).unwrap_or("stack");
                if !matches!(mode, "stack" | "switcher" | "carousel" | "wizard") {
                    return Err("mode is stack, switcher, carousel or wizard".into());
                }
                self.mode = mode.into();
            }
            "pages" => {
                self.pages = match value {
                    IpcValue::Table(t) => match t.as_ref() {
                        IpcTable::List(items) => items.iter().filter_map(|v| text(Some(v)).map(str::to_owned)).collect(),
                        _ => Vec::new(),
                    },
                    _ => Vec::new(),
                };
                if self.history.is_empty()
                    && let Some(first) = self.pages.first()
                {
                    self.history = vec![first.clone()];
                }
            }
            "current" => {
                let page = text(Some(value)).unwrap_or("").to_owned();
                let before = self.current();
                let had = !self.history.is_empty();
                // An empty stack takes the page as its root, "" included.
                if page != self.current() || self.history.is_empty() {
                    if self.mode == "stack" {
                        if let Some(at) = self.history.iter().position(|p| *p == page) {
                            self.history.truncate(at + 1);
                        } else {
                            self.history = vec![page];
                        }
                    } else {
                        self.history = vec![page];
                    }
                }
                // A binding that moves the page shows it, as `go` would.
                if had && self.current() != before {
                    let forward = self.pages.iter().position(|p| *p == self.current())
                        >= self.pages.iter().position(|p| *p == before);
                    self.direction = if forward { 1 } else { -1 };
                    self.fields_into(&mut effects);
                    effects.raise("current_changed", vec![self.current().into(), self.direction.into()]);
                }
            }
            "wrap" => self.wrap = expect_boolean(Some(value), field)?,
            _ => return Err(format!("Navigation has no setting `{field}`")),
        }
        self.fields_into(&mut effects);
        Ok(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stack_pushes_and_pops_and_alt_left_goes_back() {
        let mut nav = Navigation::new();
        nav.configure("current", &"settings".into()).unwrap();
        nav.handle("push", &["sound".into()]).unwrap();
        nav.handle("push", &["equalizer".into()]).unwrap();
        assert_eq!(nav.current(), "equalizer");
        let effects = nav.handle("key", &["Left".into(), "alt".into()]).unwrap();
        assert!(effects.handled);
        assert_eq!(nav.current(), "sound");
        nav.handle("go", &["settings".into()]).unwrap();
        assert_eq!(nav.history, vec!["settings".to_owned()]);
        assert!(!nav.handle("pop", &[]).unwrap().signals.iter().any(|(n, _)| n == "popped"), "the root stays");
    }

    #[test]
    fn a_switcher_cycles_with_ctrl_tab_and_a_carousel_with_the_arrows() {
        let pages = IpcValue::Table(std::sync::Arc::new(IpcTable::List(vec!["a".into(), "b".into(), "c".into()])));
        let mut nav = Navigation::new();
        nav.configure("mode", &"switcher".into()).unwrap();
        nav.configure("pages", &pages).unwrap();
        nav.configure("wrap", &true.into()).unwrap();
        nav.handle("key", &["Tab".into(), "ctrl".into()]).unwrap();
        assert_eq!(nav.current(), "b");
        nav.handle("key", &["ISO_Left_Tab".into(), "ctrl+shift".into()]).unwrap();
        nav.handle("key", &["ISO_Left_Tab".into(), "ctrl+shift".into()]).unwrap();
        assert_eq!(nav.current(), "c", "wrapped round");
        nav.configure("mode", &"carousel".into()).unwrap();
        nav.handle("key", &["Right".into(), "".into()]).unwrap();
        assert_eq!(nav.current(), "a");
    }
}
