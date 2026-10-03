//! `Control`, the base every archetype shares: hover, press, focus,
//! enabled, mirrored and highlighted, and the implicit size a skin's slots
//! give it.

use morf_lua::IpcValue;

use crate::value::{expect_boolean, number};
use crate::{Archetype, Effects};

/// The state every control has.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlState {
    pub hovered: bool,
    /// A press is held on it.
    pub down: bool,
    pub focused: bool,
    /// Focus came from the keyboard: the ring shows.
    pub visual_focus: bool,
    pub enabled: bool,
    /// Laid out right to left.
    pub mirrored: bool,
    /// Picked out without focus: the current row of a list under the
    /// pointer, a menu item the arrows reached.
    pub highlighted: bool,
    /// Where the last press landed, in the control: what a ripple grows from.
    pub pressed_at: (f64, f64),
}

impl Default for ControlState {
    fn default() -> Self {
        Self {
            hovered: false,
            down: false,
            focused: false,
            visual_focus: false,
            enabled: true,
            mirrored: false,
            highlighted: false,
            pressed_at: (0.0, 0.0),
        }
    }
}

impl ControlState {
    /// The fields, by name, as a skin reads them.
    pub fn fields(&self) -> Vec<(String, IpcValue)> {
        vec![
            ("hovered".into(), self.hovered.into()),
            ("down".into(), self.down.into()),
            ("focused".into(), self.focused.into()),
            ("visual_focus".into(), self.visual_focus.into()),
            ("enabled".into(), self.enabled.into()),
            ("mirrored".into(), self.mirrored.into()),
            ("highlighted".into(), self.highlighted.into()),
            ("pressed_x".into(), self.pressed_at.0.into()),
            ("pressed_y".into(), self.pressed_at.1.into()),
        ]
    }

    /// Takes a pointer, focus or setting event every control answers the
    /// same way. Returns `None` for an event that is not one of them.
    pub fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Option<Effects> {
        let mut effects = Effects::default();
        match event {
            "entered" | "exited" => {
                let hovered = event == "entered";
                if self.hovered != hovered {
                    self.hovered = hovered;
                    effects.set("hovered", hovered);
                }
            }
            "pressed" if self.enabled => {
                self.down = true;
                self.pressed_at = (
                    number(arguments.first()).unwrap_or(0.0),
                    number(arguments.get(1)).unwrap_or(0.0),
                );
                effects.set("down", true);
                effects.set("pressed_x", self.pressed_at.0);
                effects.set("pressed_y", self.pressed_at.1);
                effects.raise("pressed", arguments.to_vec());
            }
            "released" | "canceled" => {
                if self.down {
                    self.down = false;
                    effects.set("down", false);
                    if event == "released" {
                        effects.raise("released", arguments.to_vec());
                    }
                }
            }
            "focus" => {
                let focused = matches!(arguments.first(), Some(IpcValue::Boolean(true)));
                let visual = focused && matches!(arguments.get(1), Some(IpcValue::Boolean(true)));
                if self.focused != focused {
                    self.focused = focused;
                    effects.set("focused", focused);
                }
                if self.visual_focus != visual {
                    self.visual_focus = visual;
                    effects.set("visual_focus", visual);
                }
            }
            "pressed" => {}
            _ => return None,
        }
        Some(effects)
    }

    /// Takes a setting every control has. Returns `None` for one it does
    /// not.
    pub fn configure(&mut self, field: &str, value: &IpcValue) -> Option<Result<Effects, String>> {
        let target = match field {
            "enabled" => &mut self.enabled,
            "mirrored" => &mut self.mirrored,
            "highlighted" => &mut self.highlighted,
            _ => return None,
        };
        Some(expect_boolean(Some(value), field).map(|on| {
            let mut effects = Effects::default();
            if *target != on {
                *target = on;
                effects.set(field, on);
                // A control turned off lets go of a press.
                if field == "enabled" && !on && self.down {
                    self.down = false;
                    effects.set("down", false);
                }
            }
            effects
        }))
    }
}

/// A bare control: the base and nothing else, for a skin that only draws.
#[derive(Default)]
pub(crate) struct Control {
    pub(crate) base: ControlState,
}

impl Archetype for Control {
    fn name(&self) -> &'static str {
        "Control"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        self.base.fields()
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        match event {
            "clicked" => {
                let mut effects = Effects::default();
                if self.base.enabled {
                    effects.raise("clicked", arguments.to_vec());
                }
                Ok(effects)
            }
            "key" => Ok(Effects::default()),
            _ => self
                .base
                .handle(event, arguments)
                .ok_or_else(|| format!("Control has no event `{event}`")),
        }
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        self.base
            .configure(field, value)
            .unwrap_or_else(|| Err(format!("Control has no setting `{field}`")))
    }
}

/// The size a control asks for: the larger of its background with the
/// insets around it and its content with the padding around it. Each side
/// is `[left, top, right, bottom]`. Computed here, once, never by a skin.
pub fn implicit_size(
    background: (f64, f64),
    content: (f64, f64),
    padding: [f64; 4],
    insets: [f64; 4],
) -> (f64, f64) {
    let width = (background.0 + insets[0] + insets[2]).max(content.0 + padding[0] + padding[2]);
    let height = (background.1 + insets[1] + insets[3]).max(content.1 + padding[1] + padding[3]);
    (width.max(0.0), height.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_is_held_until_released_and_not_while_disabled() {
        let mut control = Control::default();
        let effects = control
            .handle("pressed", &[3.0.into(), 4.0.into()])
            .unwrap();
        assert!(effects.changed.contains(&("down".into(), true.into())));
        assert_eq!(effects.signals[0].0, "pressed");
        let effects = control.handle("released", &[]).unwrap();
        assert!(effects.changed.contains(&("down".into(), false.into())));
        control.configure("enabled", &false.into()).unwrap();
        assert!(control.handle("pressed", &[]).unwrap().changed.is_empty());
        assert!(control.handle("clicked", &[]).unwrap().signals.is_empty());
    }

    #[test]
    fn the_ring_shows_only_for_keyboard_focus() {
        let mut control = Control::default();
        control
            .handle("focus", &[true.into(), false.into()])
            .unwrap();
        assert!(control.base.focused && !control.base.visual_focus);
        control
            .handle("focus", &[true.into(), true.into()])
            .unwrap();
        assert!(control.base.visual_focus);
        control
            .handle("focus", &[false.into(), true.into()])
            .unwrap();
        assert!(!control.base.focused && !control.base.visual_focus);
    }

    #[test]
    fn implicit_size_is_the_larger_of_background_and_content() {
        assert_eq!(
            implicit_size((40.0, 20.0), (30.0, 10.0), [6.0; 4], [0.0; 4]),
            (42.0, 22.0)
        );
        assert_eq!(
            implicit_size((80.0, 40.0), (30.0, 10.0), [6.0; 4], [2.0; 4]),
            (84.0, 44.0)
        );
    }
}
