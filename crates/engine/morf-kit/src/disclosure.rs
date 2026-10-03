//! `Disclosure`: an open or closed region -- expanders, accordions,
//! collapsible sections, tree nodes, "show more".
//!
//! Settings: `expanded`, `group` (an accordion: one open at most among the
//! disclosures sharing it), `animated`.
//!
//! State: `expanded`, `animated`, and the base's.
//!
//! Events: the base's, `"clicked"` (the header pressed), `"key"` (Space and
//! Return toggle; Left collapses and Right expands, as in a tree).
//! Signals: `toggled` (expanded), `expanded`, `collapsed`.

use morf_value::IpcValue;

use crate::control::ControlState;
use crate::group::{Member, Membership};
use crate::value::{expect_boolean, text};
use crate::{Archetype, Effects};

pub(crate) struct Disclosure {
    pub(crate) base: ControlState,
    expanded: bool,
    animated: bool,
    group: Option<Membership>,
}

impl Disclosure {
    pub(crate) fn new() -> Self {
        Self { base: ControlState::default(), expanded: false, animated: true, group: None }
    }

    fn set(&mut self, expanded: bool) -> Effects {
        let mut effects = Effects::default();
        if expanded != self.expanded {
            self.expanded = expanded;
            effects.set("expanded", expanded);
            effects.set("checked", expanded);
            effects.raise("toggled", vec![expanded.into()]);
            effects.raise(if expanded { "expanded" } else { "collapsed" }, Vec::new());
        }
        effects
    }
}

impl Member for Disclosure {
    fn checked(&self) -> bool {
        self.expanded
    }
    fn set_checked_by_group(&mut self, checked: bool) -> Effects {
        self.set(checked)
    }
    fn enabled(&self) -> bool {
        self.base.enabled
    }
}

impl Archetype for Disclosure {
    fn name(&self) -> &'static str {
        "Disclosure"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.push(("expanded".into(), self.expanded.into()));
        // The same, as an accordion's group reads it.
        fields.push(("checked".into(), self.expanded.into()));
        fields.push(("animated".into(), self.animated.into()));
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        match event {
            "clicked" if self.base.enabled => {
                let next = !self.expanded;
                let mut effects = self.set(next);
                // The registry closes the rest of an accordion through
                // `checked`: say it as a check.
                if next {
                    effects.set("checked", true);
                }
                Ok(effects)
            }
            "key" if self.base.enabled => {
                let name = text(arguments.first()).unwrap_or("");
                let (collapse, expand) = if self.base.mirrored { ("Right", "Left") } else { ("Left", "Right") };
                let effects = match name {
                    "space" | "Return" | "KP_Enter" => {
                        let mut e = self.set(!self.expanded);
                        if self.expanded {
                            e.set("checked", true);
                        }
                        e
                    }
                    n if n == collapse && self.expanded => self.set(false),
                    n if n == expand && !self.expanded => {
                        let mut e = self.set(true);
                        e.set("checked", true);
                        e
                    }
                    _ => return Ok(Effects::default()),
                };
                Ok(effects.handled())
            }
            "clicked" | "key" => Ok(Effects::default()),
            _ => self.base.handle(event, arguments).ok_or_else(|| format!("Disclosure has no event `{event}`")),
        }
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        match field {
            "expanded" => {
                let on = expect_boolean(Some(value), field)?;
                let mut effects = Effects::default();
                if on != self.expanded {
                    self.expanded = on;
                    effects.set("expanded", on);
                    if on {
                        effects.set("checked", true);
                    }
                }
                Ok(effects)
            }
            "animated" => {
                self.animated = expect_boolean(Some(value), field)?;
                Ok(Effects::default())
            }
            "group" => {
                let name = text(Some(value)).ok_or("group must be a name")?.to_owned();
                self.group = Some(Membership { name, exclusive: true, allow_none: true });
                Ok(Effects::default())
            }
            _ => Err(format!("Disclosure has no setting `{field}`")),
        }
    }

    fn group(&self) -> Option<Membership> {
        self.group.clone()
    }

    fn as_member(&mut self) -> Option<&mut dyn Member> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_toggles_and_the_arrows_open_and_close() {
        let mut d = Disclosure::new();
        let effects = d.handle("clicked", &[]).unwrap();
        assert!(effects.signals.iter().any(|(n, _)| n == "expanded"));
        assert!(d.handle("key", &["Left".into(), "".into()]).unwrap().handled);
        assert!(!d.expanded);
        assert!(!d.handle("key", &["Left".into(), "".into()]).unwrap().handled, "already shut");
        d.handle("key", &["Right".into(), "".into()]).unwrap();
        assert!(d.expanded);
    }
}
