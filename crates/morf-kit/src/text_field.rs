//! `TextField`: an entry around the engine's text input (which already
//! has the caret, selection, input method and editing keys).
//!
//! Settings: `text`, `placeholder`, `echo` (`"normal"`, `"password"`,
//! `"none"`), `read_only`, `max_length`, `validator` (`"integer"`,
//! `"number"`, `"email"`, `"url"`, `"hex_color"`, or none), `minimum`,
//! `maximum` (for the numeric validators), `required`, `revert_on_escape`.
//!
//! State: `text`, `length`, `empty`, `acceptable`, `echo`, `read_only`,
//! `placeholder`, `revealed` (a password shown).
//!
//! Events: the base's, `"edited"` (text: the input changed it), `"accepted"`
//! (Return), `"escape"`, `"clear"`, `"reveal"` (toggle a password's
//! echo). Signals: `edited` and `text_changed` (text), `accepted` (text),
//! `invalid` (text, on Return when not acceptable), and `set_text` (text:
//! the Lua side writes it into the input -- a revert or a clear).
//!
//! With `capture` (a shortcut recorder) the field takes a key chord rather
//! than text: `"key"` (name, modifiers) makes its text the chord
//! (`"ctrl+shift+k"`) and raises `captured` (chord); Escape gives back the
//! one it had, BackSpace or Delete alone clears it, and a modifier pressed
//! alone waits for the key it goes with.

use morf_lua::IpcValue;

use crate::control::ControlState;
use crate::value::{expect_boolean, expect_number, number, text};
use crate::{Archetype, Effects};

pub(crate) struct TextField {
    pub(crate) base: ControlState,
    text: String,
    /// What Return last accepted: what Escape goes back to.
    accepted_text: String,
    placeholder: String,
    echo: String,
    revealed: bool,
    read_only: bool,
    max_length: usize,
    validator: Option<String>,
    minimum: Option<f64>,
    maximum: Option<f64>,
    required: bool,
    revert_on_escape: bool,
    capture: bool,
}

impl TextField {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            text: String::new(),
            accepted_text: String::new(),
            placeholder: String::new(),
            echo: "normal".into(),
            revealed: false,
            read_only: false,
            max_length: 0,
            validator: None,
            minimum: None,
            maximum: None,
            required: false,
            revert_on_escape: false,
            capture: false,
        }
    }

    /// Whether `text` passes the field's rules.
    pub(crate) fn acceptable(&self, text: &str) -> bool {
        if text.is_empty() {
            return !self.required;
        }
        if self.max_length > 0 && text.chars().count() > self.max_length {
            return false;
        }
        let in_range =
            |n: f64| self.minimum.is_none_or(|m| n >= m) && self.maximum.is_none_or(|m| n <= m);
        match self.validator.as_deref() {
            None => true,
            Some("integer") => text.trim().parse::<i64>().is_ok_and(|n| in_range(n as f64)),
            Some("number") => text
                .trim()
                .parse::<f64>()
                .is_ok_and(|n| n.is_finite() && in_range(n)),
            Some("email") => {
                let mut parts = text.split('@');
                match (parts.next(), parts.next(), parts.next()) {
                    (Some(user), Some(host), None) => {
                        !user.is_empty()
                            && host.contains('.')
                            && !host.starts_with('.')
                            && !host.ends_with('.')
                    }
                    _ => false,
                }
            }
            Some("url") => text.split_once("://").is_some_and(|(scheme, rest)| {
                !scheme.is_empty()
                    && scheme
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '+')
                    && !rest.is_empty()
            }),
            Some("hex_color") => {
                let hex = text.strip_prefix('#').unwrap_or(text);
                matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
            }
            Some(_) => true,
        }
    }

    fn fields_into(&self, effects: &mut Effects) {
        for (field, value) in self.text_fields() {
            effects.set(&field, value);
        }
    }

    fn text_fields(&self) -> Vec<(String, IpcValue)> {
        vec![
            ("text".into(), self.text.clone().into()),
            ("length".into(), (self.text.chars().count() as i64).into()),
            ("empty".into(), self.text.is_empty().into()),
            ("acceptable".into(), self.acceptable(&self.text).into()),
            ("echo".into(), self.shown_echo().into()),
            ("revealed".into(), self.revealed.into()),
        ]
    }

    /// The echo the input uses now: a revealed password shows.
    fn shown_echo(&self) -> String {
        if self.echo == "password" && self.revealed {
            "normal".into()
        } else {
            self.echo.clone()
        }
    }

    fn set_text(&mut self, text: String, effects: &mut Effects) {
        if text != self.text {
            self.text = text.clone();
            self.fields_into(effects);
            effects.raise("text_changed", vec![text.into()]);
        }
    }
}

impl Archetype for TextField {
    fn name(&self) -> &'static str {
        "TextField"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.text_fields());
        fields.push(("placeholder".into(), self.placeholder.clone().into()));
        fields.push(("read_only".into(), self.read_only.into()));
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let mut effects = Effects::default();
        match event {
            "edited" => {
                let text = text(arguments.first()).unwrap_or("").to_owned();
                if text != self.text {
                    self.set_text(text.clone(), &mut effects);
                    effects.raise("edited", vec![text.into()]);
                }
            }
            "accepted" => {
                if self.acceptable(&self.text) {
                    self.accepted_text = self.text.clone();
                    effects.raise("accepted", vec![self.text.clone().into()]);
                } else {
                    effects.raise("invalid", vec![self.text.clone().into()]);
                }
            }
            "escape" => {
                if self.revert_on_escape && self.text != self.accepted_text {
                    let back = self.accepted_text.clone();
                    self.set_text(back.clone(), &mut effects);
                    effects.raise("set_text", vec![back.into()]);
                    effects.handled = true;
                }
            }
            "clear" if !self.read_only && self.base.enabled => {
                self.set_text(String::new(), &mut effects);
                effects.raise("set_text", vec!["".into()]);
                effects.raise("edited", vec!["".into()]);
            }
            "reveal" if self.echo == "password" => {
                self.revealed = !self.revealed;
                self.fields_into(&mut effects);
            }
            "key" if self.capture && self.base.enabled => {
                let name = text(arguments.first()).unwrap_or("").to_owned();
                let modifiers = text(arguments.get(1)).unwrap_or("").to_owned();
                const ALONE: &[&str] = &["Shift_L", "Shift_R", "Control_L", "Control_R", "Alt_L", "Alt_R",
                    "Super_L", "Super_R", "Meta_L", "Meta_R", "ISO_Level3_Shift", "Caps_Lock"];
                effects.handled = true;
                if name.is_empty() || ALONE.contains(&name.as_str()) {
                    return Ok(effects);
                }
                let chord = match (name.as_str(), modifiers.is_empty()) {
                    ("Escape", true) => {
                        let back = self.accepted_text.clone();
                        self.set_text(back.clone(), &mut effects);
                        effects.raise("set_text", vec![back.into()]);
                        return Ok(effects);
                    }
                    ("BackSpace" | "Delete", true) => String::new(),
                    // Tab alone still leaves: a recorder must not trap focus.
                    ("Tab" | "ISO_Left_Tab", true) => {
                        effects.handled = false;
                        return Ok(effects);
                    }
                    _ => {
                        let key = if name.chars().count() == 1 { name.to_lowercase() } else { name.clone() };
                        if modifiers.is_empty() { key } else { format!("{modifiers}+{key}") }
                    }
                };
                self.set_text(chord.clone(), &mut effects);
                self.accepted_text = chord.clone();
                effects.raise("set_text", vec![chord.clone().into()]);
                effects.raise("edited", vec![chord.clone().into()]);
                effects.raise("captured", vec![chord.into()]);
            }
            "clear" | "reveal" | "clicked" | "key" => {}
            _ => {
                return self
                    .base
                    .handle(event, arguments)
                    .ok_or_else(|| format!("TextField has no event `{event}`"));
            }
        }
        Ok(effects)
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        let mut effects = Effects::default();
        match field {
            "text" => {
                let text = text(Some(value)).unwrap_or("").to_owned();
                self.accepted_text = text.clone();
                if text != self.text {
                    self.text = text;
                    self.fields_into(&mut effects);
                }
            }
            "placeholder" => {
                self.placeholder = text(Some(value)).unwrap_or("").to_owned();
                effects.set("placeholder", self.placeholder.clone());
            }
            "echo" => {
                let echo = text(Some(value)).unwrap_or("normal");
                if !matches!(echo, "normal" | "password" | "none") {
                    return Err("echo is normal, password or none".into());
                }
                self.echo = echo.into();
                self.fields_into(&mut effects);
            }
            "read_only" => {
                self.read_only = expect_boolean(Some(value), field)?;
                effects.set("read_only", self.read_only);
            }
            "max_length" => {
                self.max_length = expect_number(Some(value), field)?.max(0.0) as usize;
                self.fields_into(&mut effects);
            }
            "validator" => {
                self.validator = text(Some(value))
                    .filter(|v| !v.is_empty() && *v != "none")
                    .map(str::to_owned);
                self.fields_into(&mut effects);
            }
            "minimum" => self.minimum = number(Some(value)),
            "maximum" => self.maximum = number(Some(value)),
            "required" => {
                self.required = expect_boolean(Some(value), field)?;
                self.fields_into(&mut effects);
            }
            "revert_on_escape" => self.revert_on_escape = expect_boolean(Some(value), field)?,
            "capture" => self.capture = expect_boolean(Some(value), field)?,
            _ => return Err(format!("TextField has no setting `{field}`")),
        }
        Ok(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(settings: &[(&str, IpcValue)]) -> TextField {
        let mut f = TextField::new();
        for (k, v) in settings {
            f.configure(k, v).unwrap();
        }
        f
    }

    #[test]
    fn validators_say_what_is_acceptable() {
        let port = field(&[
            ("validator", "integer".into()),
            ("minimum", 1.0.into()),
            ("maximum", 65535.0.into()),
        ]);
        assert!(port.acceptable("8080") && !port.acceptable("0") && !port.acceptable("http"));
        let email = field(&[("validator", "email".into())]);
        assert!(
            email.acceptable("a@b.io") && !email.acceptable("a@b") && !email.acceptable("@b.io")
        );
        let colour = field(&[("validator", "hex_color".into())]);
        assert!(colour.acceptable("#ef5350") && !colour.acceptable("#ef535"));
        assert!(!field(&[("required", true.into())]).acceptable(""));
    }

    #[test]
    fn return_accepts_only_what_passes_and_escape_reverts() {
        let mut f = field(&[
            ("validator", "number".into()),
            ("revert_on_escape", true.into()),
            ("text", "1".into()),
        ]);
        f.handle("edited", &["2.5".into()]).unwrap();
        assert_eq!(f.handle("accepted", &[]).unwrap().signals[0].0, "accepted");
        f.handle("edited", &["x".into()]).unwrap();
        assert_eq!(f.handle("accepted", &[]).unwrap().signals[0].0, "invalid");
        let effects = f.handle("escape", &[]).unwrap();
        assert!(
            effects
                .signals
                .iter()
                .any(|(n, a)| n == "set_text" && a == &vec![IpcValue::from("2.5")])
        );
    }

    #[test]
    fn a_password_can_be_revealed() {
        let mut f = field(&[("echo", "password".into())]);
        f.handle("reveal", &[]).unwrap();
        assert_eq!(f.shown_echo(), "normal");
    }
}
