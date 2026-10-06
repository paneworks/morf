//! Input for a headless configuration: events the headless backend's seat
//! carries to the host, as a compositor's would, through the shell's own
//! pointer and key paths.
//!
//! Split from `headless` at the line gate.

use morf_app::Backend;
use std::time::Duration;

use morf_app::backend::headless::VirtualSeat;
use morf_app::{Event, WindowId};

use crate::headless::Headless;

impl Headless {
    /// Hands one event to the host through the backend's seat, then lets a
    /// frame's worth of nothing pass so the layout shows what it did.
    pub fn send(&mut self, event: Event) -> Result<(), String> {
        let backend = self
            .host
            .as_mut()
            .and_then(|host| host.backend.as_headless_mut())
            .ok_or_else(|| "the configuration has no surface to send input to".to_owned())?;
        backend.inject(event);
        self.frame(Duration::ZERO);
        Ok(())
    }

    /// One pointer or touch event.
    pub fn pointer(&mut self, event: Event) -> Result<(), String> {
        match event {
            Event::PointerMotion { .. }
            | Event::PointerLeave { .. }
            | Event::PointerButton { .. }
            | Event::PointerAxis { .. }
            | Event::TouchDown { .. }
            | Event::TouchMotion { .. }
            | Event::TouchUp { .. }
            | Event::TouchCancel => self.send(event),
            event => Err(format!("not a pointer event: {event:?}")),
        }
    }

    /// Presses and releases a button at a point of a surface: a click.
    pub fn click(
        &mut self,
        surface: WindowId,
        (x, y): (f64, f64),
        button: u32,
        modifiers: morf_app::KeyModifiers,
    ) -> Result<(), String> {
        for event in VirtualSeat::click(surface, (x, y), button, modifiers) {
            self.pointer(event)?;
        }
        Ok(())
    }

    /// One key, pressed and released, into a surface's focused node.
    pub fn key(
        &mut self,
        surface: WindowId,
        keysym: u32,
        text: Option<&str>,
        modifiers: morf_lua::KeyModifiers,
    ) -> Result<(), String> {
        self.key_phase(surface, keysym, text, modifiers, true, true)
    }

    /// A key pressed, let go, or both: a modifier held across other keys.
    pub fn key_phase(
        &mut self,
        surface: WindowId,
        keysym: u32,
        text: Option<&str>,
        modifiers: morf_lua::KeyModifiers,
        press: bool,
        release: bool,
    ) -> Result<(), String> {
        let modifiers = morf_app::KeyModifiers {
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            logo: modifiers.logo,
        };
        for pressed in [press.then_some(true), release.then_some(false)]
            .into_iter()
            .flatten()
        {
            self.send(Event::Key {
                surface,
                keysym,
                text: text.map(str::to_owned),
                pressed,
                repeat: false,
                modifiers,
            })?;
        }
        Ok(())
    }
}

/// The keysym a key name stands for, and the text it types.
///
/// X keysym names, the ones `xkbcommon` uses (`Return`, `Escape`, `Left`,
/// `F5`), a few friendlier spellings (`Enter`, `Esc`), and any single
/// character, whose keysym is its code point the way X assigns them.
pub fn keysym(name: &str) -> Option<(u32, Option<String>)> {
    let keysym = morf_lua::keys::keysym(name)?;
    // What the key types, as a keyboard's would.
    let text = match keysym {
        0xff0d => Some("\r".to_owned()),
        0xff09 => Some("\t".to_owned()),
        0x20 => Some(" ".to_owned()),
        _ => {
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(only), None) => Some(only.to_string()),
                _ => None,
            }
        }
    };
    Some((keysym, text))
}

/// Reads the modifier names a key is pressed with.
pub fn modifiers(names: &[String]) -> Result<morf_lua::KeyModifiers, String> {
    let mut modifiers = morf_lua::KeyModifiers::default();
    for name in names {
        match name.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.ctrl = true,
            "shift" => modifiers.shift = true,
            "alt" | "mod1" => modifiers.alt = true,
            "logo" | "super" | "mod4" | "meta" => modifiers.logo = true,
            other => return Err(format!("unknown modifier `{other}`")),
        }
    }
    Ok(modifiers)
}

/// The modifiers a pointer event is sent with, as `"ctrl+shift"`.
pub fn pointer_modifiers(names: Option<&str>) -> Result<morf_app::KeyModifiers, String> {
    let names: Vec<String> = names
        .unwrap_or("")
        .split(['+', ',', ' '])
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    let held = modifiers(&names)?;
    Ok(morf_app::KeyModifiers {
        ctrl: held.ctrl,
        shift: held.shift,
        alt: held.alt,
        logo: held.logo,
    })
}

/// A mouse button by name or Linux event code.
pub fn button(name: &str) -> Result<u32, String> {
    match name {
        "left" | "1" => Ok(0x110),
        "right" | "3" => Ok(0x111),
        "middle" | "2" => Ok(0x112),
        "back" | "side" => Ok(0x113),
        "forward" | "extra" => Ok(0x114),
        other => other
            .parse::<u32>()
            .ok()
            .filter(|code| *code >= 0x110)
            .ok_or_else(|| format!("unknown mouse button `{other}`")),
    }
}
