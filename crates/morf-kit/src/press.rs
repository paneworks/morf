//! `Press`: everything that is pressed -- buttons, toggles, switches,
//! checkboxes, radio buttons, chips, menu items.
//!
//! Settings: `checkable`, `checked`, `tristate` and `partial` (a checkbox's
//! third state), `group`, `exclusive` and `allow_none` (see `group.rs`),
//! `auto_repeat` with `repeat_delay` and `repeat_interval` (ms), and the
//! base's `enabled`, `mirrored`, `highlighted`.
//!
//! Events: the base's pointer and focus events, `"clicked"`, `"key"` (name,
//! modifiers), `"repeat"` (the Lua side's timer), `"long_pressed"`,
//! `"double_clicked"`. Signals: `pressed`, `released`, `clicked`,
//! `toggled` (checked), `long_pressed`, `double_clicked`, and two the Lua
//! side acts on: `schedule_repeat` (ms) and `focus_request` (on a group
//! member the arrows moved to).

use morf_lua::IpcValue;

use crate::control::ControlState;
use crate::group::{Member, Membership};
use crate::value::{expect_boolean, expect_number, text};
use crate::{Archetype, Effects};

#[derive(Default)]
pub(crate) struct Press {
    pub(crate) base: ControlState,
    checkable: bool,
    checked: bool,
    tristate: bool,
    partial: bool,
    group: Option<Membership>,
    auto_repeat: bool,
    repeat_delay: f64,
    repeat_interval: f64,
    /// Repeats fired during the press under way: a release after one is not
    /// also a click.
    repeated: bool,
}

impl Press {
    pub(crate) fn new() -> Self {
        Self {
            repeat_delay: 400.0,
            repeat_interval: 60.0,
            ..Self::default()
        }
    }

    /// What a click does: toggles a checkable press, then says so.
    fn activate(&mut self) -> Effects {
        let mut effects = Effects::default();
        if !self.base.enabled {
            return effects;
        }
        if self.checkable {
            let locked = self.checked
                && self
                    .group
                    .as_ref()
                    .is_some_and(|g| g.exclusive && !g.allow_none);
            if !locked {
                let checked = if self.partial { true } else { !self.checked };
                if self.partial {
                    self.partial = false;
                    effects.set("partial", false);
                }
                if checked != self.checked {
                    self.checked = checked;
                    effects.set("checked", checked);
                    effects.raise("toggled", vec![checked.into()]);
                }
            }
        }
        effects.raise("clicked", Vec::new());
        effects
    }
}

impl Member for Press {
    fn checked(&self) -> bool {
        self.checked
    }

    fn set_checked_by_group(&mut self, checked: bool) -> Effects {
        let mut effects = Effects::default();
        if self.checked != checked {
            self.checked = checked;
            effects.set("checked", checked);
            effects.raise("toggled", vec![checked.into()]);
        }
        effects
    }

    fn enabled(&self) -> bool {
        self.base.enabled
    }
}

impl Archetype for Press {
    fn name(&self) -> &'static str {
        "Press"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend([
            ("checkable".into(), self.checkable.into()),
            ("checked".into(), self.checked.into()),
            ("partial".into(), self.partial.into()),
        ]);
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        match event {
            "clicked" => {
                if std::mem::take(&mut self.repeated) {
                    return Ok(Effects::default());
                }
                Ok(self.activate())
            }
            "pressed" => {
                let mut effects = self.base.handle(event, arguments).unwrap_or_default();
                self.repeated = false;
                if self.auto_repeat && self.base.enabled {
                    effects.raise("schedule_repeat", vec![self.repeat_delay.into()]);
                }
                Ok(effects)
            }
            "repeat" => {
                let mut effects = Effects::default();
                if self.base.down && self.auto_repeat && self.base.enabled {
                    self.repeated = true;
                    effects.raise("clicked", Vec::new());
                    effects.raise("schedule_repeat", vec![self.repeat_interval.into()]);
                }
                Ok(effects)
            }
            "long_pressed" | "double_clicked" => {
                let mut effects = Effects::default();
                if self.base.enabled {
                    effects.raise(event, arguments.to_vec());
                }
                Ok(effects)
            }
            "key" => {
                let name = text(arguments.first()).unwrap_or("");
                let modifiers = text(arguments.get(1)).unwrap_or("");
                let plain = modifiers.is_empty() || modifiers == "shift";
                match name {
                    "space" | "Return" | "KP_Enter" if plain && self.base.enabled => {
                        Ok(self.activate().handled())
                    }
                    // The arrows within an exclusive group: the registry
                    // moves to the neighbour (see `module.rs`).
                    _ => Ok(Effects::default()),
                }
            }
            _ => self
                .base
                .handle(event, arguments)
                .ok_or_else(|| format!("Press has no event `{event}`")),
        }
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        let mut effects = Effects::default();
        let flag = |what| expect_boolean(Some(value), what);
        match field {
            "checkable" => {
                self.checkable = flag(field)?;
                effects.set(field, self.checkable);
            }
            "checked" => {
                let checked = flag(field)?;
                if checked != self.checked {
                    self.checked = checked;
                    effects.set(field, checked);
                }
            }
            "tristate" => self.tristate = flag(field)?,
            "partial" => {
                // Saying partial is saying it has a third state.
                let partial = flag(field)?;
                self.tristate |= partial;
                if partial != self.partial {
                    self.partial = partial;
                    effects.set(field, partial);
                }
            }
            "group" => {
                let name = text(Some(value)).ok_or("group must be a name")?.to_owned();
                let group = self.group.get_or_insert(Membership {
                    name: String::new(),
                    exclusive: true,
                    allow_none: false,
                });
                group.name = name;
            }
            "exclusive" | "allow_none" => {
                let on = flag(field)?;
                let group = self.group.get_or_insert(Membership {
                    name: String::new(),
                    exclusive: true,
                    allow_none: false,
                });
                if field == "exclusive" {
                    group.exclusive = on
                } else {
                    group.allow_none = on
                }
            }
            "auto_repeat" => self.auto_repeat = flag(field)?,
            "repeat_delay" => self.repeat_delay = expect_number(Some(value), field)?.max(0.0),
            "repeat_interval" => self.repeat_interval = expect_number(Some(value), field)?.max(1.0),
            _ => return Err(format!("Press has no setting `{field}`")),
        }
        Ok(effects)
    }

    fn group(&self) -> Option<Membership> {
        self.group.clone().filter(|g| !g.name.is_empty())
    }

    fn as_member(&mut self) -> Option<&mut dyn Member> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(settings: &[(&str, IpcValue)]) -> Press {
        let mut press = Press::new();
        for (field, value) in settings {
            press.configure(field, value).unwrap();
        }
        press
    }

    #[test]
    fn a_checkable_press_toggles_and_a_plain_one_only_clicks() {
        let mut plain = press(&[]);
        let effects = plain.handle("clicked", &[]).unwrap();
        assert_eq!(effects.signals.len(), 1);
        let mut toggle = press(&[("checkable", true.into())]);
        let effects = toggle.handle("clicked", &[]).unwrap();
        assert!(effects.changed.contains(&("checked".into(), true.into())));
        assert_eq!(effects.signals[0], ("toggled".into(), vec![true.into()]));
    }

    #[test]
    fn a_partial_checkbox_goes_to_checked() {
        let mut checkbox = press(&[
            ("checkable", true.into()),
            ("tristate", true.into()),
            ("partial", true.into()),
        ]);
        checkbox.handle("clicked", &[]).unwrap();
        assert!(checkbox.checked && !checkbox.partial);
    }

    #[test]
    fn space_and_return_activate_and_other_keys_pass_on() {
        let mut button = press(&[]);
        assert!(
            button
                .handle("key", &["space".into(), "".into()])
                .unwrap()
                .handled
        );
        assert!(
            !button
                .handle("key", &["a".into(), "".into()])
                .unwrap()
                .handled
        );
        assert!(
            !button
                .handle("key", &["Return".into(), "ctrl".into()])
                .unwrap()
                .handled
        );
    }

    #[test]
    fn an_exclusive_member_stays_checked_when_pressed_again() {
        let mut radio = press(&[("checkable", true.into()), ("group", "g".into())]);
        radio.handle("clicked", &[]).unwrap();
        radio.handle("clicked", &[]).unwrap();
        assert!(radio.checked);
    }

    #[test]
    fn auto_repeat_clicks_while_held_and_not_again_on_release() {
        let mut stepper = press(&[("auto_repeat", true.into())]);
        let effects = stepper.handle("pressed", &[]).unwrap();
        assert!(
            effects
                .signals
                .iter()
                .any(|(name, _)| name == "schedule_repeat")
        );
        let effects = stepper.handle("repeat", &[]).unwrap();
        assert_eq!(effects.signals[0].0, "clicked");
        stepper.handle("released", &[]).unwrap();
        assert!(stepper.handle("clicked", &[]).unwrap().signals.is_empty());
    }
}
