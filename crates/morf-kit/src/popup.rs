//! `Popup`: what floats over a surface in its overlay layer -- menus,
//! tooltips, dialogs, toasts, sheets, and a shell's drawers.
//!
//! The engine's overlay layer does the floating: the stack, placement,
//! Escape, a press outside, the scrim, focus in and back. The archetype
//! keeps whether it is open and its policy, and says what opening and
//! closing do.
//!
//! Settings: `modal`, `dim`, `close_policy` (`"escape"`, `"outside"`,
//! both joined by `+`, or `"none"`), `placement` (as the overlay's:
//! `"bottom-start"`, `"center"`, ...), `focus_on_open`, `restore_focus`.
//!
//! State: `open`, `modal`, `dim`, `escape` and `outside` (the policy,
//! split), `placement`, `focus_on_open`.
//!
//! Events: `"open"`, `"close"` (reason: `"escape"`, `"outside"`,
//! `"closed"`, `"gone"`), `"key"`. Signals: `opened`, `closed` (reason).

use morf_lua::IpcValue;

use crate::control::ControlState;
use crate::value::{expect_boolean, text};
use crate::{Archetype, Effects};

pub(crate) struct Popup {
    pub(crate) base: ControlState,
    open: bool,
    modal: bool,
    dim: bool,
    escape: bool,
    outside: bool,
    placement: String,
    focus_on_open: bool,
    restore_focus: bool,
}

impl Popup {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            open: false,
            modal: false,
            dim: false,
            escape: true,
            outside: true,
            placement: "bottom-start".into(),
            focus_on_open: true,
            restore_focus: true,
        }
    }
}

impl Archetype for Popup {
    fn name(&self) -> &'static str {
        "Popup"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend([
            ("open".into(), self.open.into()),
            ("modal".into(), self.modal.into()),
            ("dim".into(), self.dim.into()),
            ("escape".into(), self.escape.into()),
            ("outside".into(), self.outside.into()),
            ("placement".into(), self.placement.clone().into()),
            ("focus_on_open".into(), self.focus_on_open.into()),
            ("restore_focus".into(), self.restore_focus.into()),
        ]);
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let mut effects = Effects::default();
        match event {
            "open" if !self.open => {
                self.open = true;
                effects.set("open", true);
                effects.raise("opened", Vec::new());
            }
            "close" if self.open => {
                self.open = false;
                effects.set("open", false);
                let reason = text(arguments.first()).unwrap_or("closed").to_owned();
                effects.raise("closed", vec![reason.into()]);
            }
            "open" | "close" | "clicked" | "key" => {}
            _ => {
                return self
                    .base
                    .handle(event, arguments)
                    .ok_or_else(|| format!("Popup has no event `{event}`"));
            }
        }
        Ok(effects)
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        let mut effects = Effects::default();
        let flag = || expect_boolean(Some(value), field);
        match field {
            "modal" => self.modal = flag()?,
            "dim" => self.dim = flag()?,
            "focus_on_open" => self.focus_on_open = flag()?,
            "restore_focus" => self.restore_focus = flag()?,
            "placement" => {
                self.placement = text(Some(value))
                    .ok_or("placement must be a name")?
                    .to_owned()
            }
            "close_policy" => {
                let policy = text(Some(value)).ok_or("close_policy must be a name")?;
                self.escape = policy.split('+').any(|p| p == "escape");
                self.outside = policy
                    .split('+')
                    .any(|p| p == "outside" || p == "outside_press");
                if !(self.escape || self.outside || policy == "none") {
                    return Err("close_policy is escape, outside, escape+outside or none".into());
                }
                effects.set("escape", self.escape);
                effects.set("outside", self.outside);
                return Ok(effects);
            }
            "open" => {
                let on = flag()?;
                return self.handle(if on { "open" } else { "close" }, &["closed".into()]);
            }
            _ => return Err(format!("Popup has no setting `{field}`")),
        }
        for (f, v) in self.state() {
            if matches!(
                f.as_str(),
                "modal" | "dim" | "placement" | "focus_on_open" | "restore_focus"
            ) {
                effects.set(&f, v);
            }
        }
        Ok(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_once_and_closes_with_its_reason() {
        let mut popup = Popup::new();
        assert_eq!(popup.handle("open", &[]).unwrap().signals[0].0, "opened");
        assert!(popup.handle("open", &[]).unwrap().signals.is_empty());
        let effects = popup.handle("close", &["escape".into()]).unwrap();
        assert_eq!(effects.signals[0], ("closed".into(), vec!["escape".into()]));
    }

    #[test]
    fn the_policy_splits_into_escape_and_outside() {
        let mut popup = Popup::new();
        popup.configure("close_policy", &"escape".into()).unwrap();
        assert!(popup.escape && !popup.outside);
        popup.configure("close_policy", &"none".into()).unwrap();
        assert!(!popup.escape && !popup.outside);
        assert!(
            popup
                .configure("close_policy", &"sometimes".into())
                .is_err()
        );
    }
}
