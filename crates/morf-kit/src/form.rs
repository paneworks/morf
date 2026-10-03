//! `Form`: what a group of fields adds up to -- whether it can be sent,
//! whether anything was changed, what is wrong and where -- and the
//! sending: a preferences page, a sign-in, a dialog's fields, a wizard
//! step. Its fields are controls of their own; each tells the form how it
//! stands.
//!
//! Settings: `submit_on_enter` (true: Return in a single-line field sends),
//! `show_errors` (`"touched"` -- a field's error shows once it was left
//! --, `"submit"` -- only after a try --, `"always"`).
//!
//! State: `valid`, `dirty`, `pending` (a field still checking), `submitting`,
//! `error_count`, `tried` (a submit was attempted), `first_invalid` (a
//! field's name, or "").
//!
//! Events: the base's; `"field"` (name, valid, dirty, message) whenever a
//! field's standing changes; `"touched"` (name: it lost focus once);
//! `"pending"` (name, bool); `"remove"` (name); `"submit"`; `"done"` (ok,
//! message: the configuration finished sending); `"reset"`; `"key"`
//! (name, modifiers): Ctrl+Return sends, Escape resets a dirty form when
//! asked. Signals: `submitted`, `invalid` (first field's name),
//! `reset`, `validity_changed` (valid), `dirty_changed` (dirty),
//! `show_error` (name, message) / `hide_error` (name) as the policy
//! decides a field's message should show.

use morf_lua::IpcValue;

use crate::control::ControlState;
use crate::value::{boolean, expect_boolean, text};
use crate::{Archetype, Effects};

#[derive(Clone, Debug, Default, PartialEq)]
struct Field {
    name: String,
    valid: bool,
    dirty: bool,
    message: String,
    touched: bool,
    pending: bool,
    shown: bool,
}

pub(crate) struct Form {
    pub(crate) base: ControlState,
    fields: Vec<Field>,
    submit_on_enter: bool,
    show: String,
    tried: bool,
    submitting: bool,
}

impl Form {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            fields: Vec::new(),
            submit_on_enter: true,
            show: "touched".into(),
            tried: false,
            submitting: false,
        }
    }

    fn valid(&self) -> bool {
        self.fields.iter().all(|f| f.valid)
    }

    fn dirty(&self) -> bool {
        self.fields.iter().any(|f| f.dirty)
    }

    fn pending(&self) -> bool {
        self.fields.iter().any(|f| f.pending)
    }

    fn first_invalid(&self) -> String {
        self.fields.iter().find(|f| !f.valid).map(|f| f.name.clone()).unwrap_or_default()
    }

    fn state_fields(&self) -> Vec<(String, IpcValue)> {
        vec![
            ("valid".into(), self.valid().into()),
            ("dirty".into(), self.dirty().into()),
            ("pending".into(), self.pending().into()),
            ("submitting".into(), self.submitting.into()),
            ("error_count".into(), (self.fields.iter().filter(|f| !f.valid).count() as i64).into()),
            ("tried".into(), self.tried.into()),
            ("first_invalid".into(), self.first_invalid().into()),
            ("submit_on_enter".into(), self.submit_on_enter.into()),
        ]
    }

    fn field(&mut self, name: &str) -> &mut Field {
        if let Some(i) = self.fields.iter().position(|f| f.name == name) {
            return &mut self.fields[i];
        }
        self.fields.push(Field { name: name.to_owned(), valid: true, ..Field::default() });
        self.fields.last_mut().expect("just pushed")
    }

    /// Which messages show now, by the policy: said as they change.
    fn messages(&mut self, effects: &mut Effects) {
        let tried = self.tried;
        let policy = self.show.clone();
        for f in &mut self.fields {
            let due = !f.valid
                && match policy.as_str() {
                    "always" => true,
                    "submit" => tried,
                    _ => tried || f.touched,
                };
            if due && !f.shown {
                effects.raise("show_error", vec![f.name.as_str().into(), f.message.as_str().into()]);
                f.shown = true;
            } else if !due && f.shown {
                f.shown = false;
                effects.raise("hide_error", vec![f.name.as_str().into()]);
            }
        }
    }

    fn changing(&mut self, change: impl FnOnce(&mut Self, &mut Effects)) -> Effects {
        let before = self.state_fields();
        let (valid, dirty) = (self.valid(), self.dirty());
        let mut inner = Effects::default();
        change(self, &mut inner);
        self.messages(&mut inner);
        let mut out = Effects::default();
        for (name, value) in self.state_fields() {
            if before.iter().find(|(n, _)| *n == name).map(|(_, v)| v) != Some(&value) {
                out.set(&name, value);
            }
        }
        if self.valid() != valid {
            out.raise("validity_changed", vec![self.valid().into()]);
        }
        if self.dirty() != dirty {
            out.raise("dirty_changed", vec![self.dirty().into()]);
        }
        out.signals.extend(inner.signals);
        out.handled = inner.handled;
        out
    }

    fn submit(&mut self, effects: &mut Effects) {
        if self.submitting {
            return;
        }
        self.tried = true;
        if self.pending() {
            return;
        }
        if self.valid() {
            self.submitting = true;
            effects.raise("submitted", Vec::new());
        } else {
            effects.raise("invalid", vec![self.first_invalid().into()]);
        }
    }
}

impl Archetype for Form {
    fn name(&self) -> &'static str {
        "Form"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.state_fields());
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let name = text(arguments.first()).unwrap_or("").to_owned();
        Ok(match event {
            "field" => {
                let valid = boolean(arguments.get(1)).unwrap_or(true);
                let dirty = boolean(arguments.get(2)).unwrap_or(false);
                let message = text(arguments.get(3)).unwrap_or("").to_owned();
                self.changing(|s, e| {
                    let f = s.field(&name);
                    let changed_message = f.message != message;
                    f.valid = valid;
                    f.dirty = dirty;
                    f.message = message.clone();
                    // A message that changed while shown is said again.
                    if f.shown && changed_message && !valid {
                        e.raise("show_error", vec![name.as_str().into(), message.into()]);
                    }
                })
            }
            "touched" => self.changing(|s, _| s.field(&name).touched = true),
            "pending" => {
                let on = boolean(arguments.get(1)).unwrap_or(false);
                self.changing(|s, _| s.field(&name).pending = on)
            }
            "remove" => self.changing(|s, _| s.fields.retain(|f| f.name != name)),
            "submit" => self.changing(|s, e| s.submit(e)),
            "done" => self.changing(|s, _| s.submitting = false),
            "reset" => self.changing(|s, e| {
                s.tried = false;
                s.submitting = false;
                for f in &mut s.fields {
                    f.touched = false;
                }
                e.raise("reset", Vec::new());
            }),
            "key" if self.base.enabled => {
                let modifiers = text(arguments.get(1)).unwrap_or("").to_owned();
                let enter = matches!(name.as_str(), "Return" | "KP_Enter");
                let mut effects = if enter && (modifiers.contains("ctrl") || self.submit_on_enter) {
                    self.changing(|s, e| s.submit(e))
                } else {
                    return Ok(Effects::default());
                };
                effects.handled = true;
                effects
            }
            "clicked" | "key" => Effects::default(),
            _ => self.base.handle(event, arguments).unwrap_or_default(),
        })
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        Ok(match field {
            "submit_on_enter" => {
                let on = expect_boolean(Some(value), field)?;
                self.changing(|s, _| s.submit_on_enter = on)
            }
            "show_errors" => {
                let policy = match text(Some(value)) {
                    Some(p @ ("touched" | "submit" | "always")) => p.to_owned(),
                    _ => return Err("show_errors is touched, submit or always".into()),
                };
                self.changing(|s, _| s.show = policy)
            }
            _ => return Err(format!("Form has no setting `{field}`")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(e: &Effects, name: &str) -> bool {
        e.signals.iter().any(|(n, _)| n == name)
    }

    #[test]
    fn a_form_sends_only_when_valid_and_shows_errors_by_policy() {
        let mut f = Form::new();
        f.handle("field", &["email".into(), false.into(), true.into(), "Not an address".into()]).unwrap();
        f.handle("field", &["name".into(), true.into(), true.into(), "".into()]).unwrap();
        assert!(!f.valid() && f.dirty());
        // Not yet touched: the message waits.
        let e = f.handle("touched", &["email".into()]).unwrap();
        assert!(has(&e, "show_error"));
        let e = f.handle("submit", &[]).unwrap();
        assert!(e.signals.iter().any(|(n, a)| n == "invalid" && a[0] == "email".into()));
        let e = f.handle("field", &["email".into(), true.into(), true.into(), "".into()]).unwrap();
        assert!(has(&e, "hide_error") && has(&e, "validity_changed"));
        let e = f.handle("key", &["Return".into(), "".into()]).unwrap();
        assert!(e.handled && has(&e, "submitted"));
        // A second Return while sending does nothing.
        assert!(!has(&f.handle("submit", &[]).unwrap(), "submitted"));
        f.handle("done", &[true.into()]).unwrap();
        f.handle("pending", &["name".into(), true.into()]).unwrap();
        assert!(!has(&f.handle("submit", &[]).unwrap(), "submitted"), "a check under way holds it");
    }
}
