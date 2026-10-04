//! Input for a headless configuration: the shell's own pointer and key
//! paths, fed with events nothing real produced.
//!
//! Split from `headless` at the line gate.

use std::time::Duration;

use morf_layout::Layout;
use morf_app::{LayerEvent, SurfaceRole};

use crate::headless::{Headless, Surface};
use crate::pointer_cursor::CursorShapes;
use crate::surface_keys::{KeyAction, dispatch_key_in_subtree};
use crate::surface_pointer::handle_pointer_event;
use crate::surfaces::SurfaceLayouts;

/// The pointer shapes a configuration asks for, which nothing here shows.
struct NoCursor;

impl CursorShapes for NoCursor {
    fn set_cursor_shape(&mut self, _shape: &str) {}
}

/// Each surface's last layout, by the role events carry.
pub(crate) struct Layouts<'a>(pub(crate) &'a [Surface]);

impl SurfaceLayouts for Layouts<'_> {
    fn layout_of(&self, surface: SurfaceRole) -> Option<&Layout> {
        self.0
            .iter()
            .find(|candidate| candidate.role == surface && candidate.visible)
            .and_then(|candidate| candidate.layout.as_ref())
    }
}

impl Headless {
    /// Hands one pointer event to the shell's own pointer path, then lets a
    /// frame's worth of nothing pass so the layout shows what it did.
    pub(crate) fn pointer(&mut self, event: LayerEvent) -> Result<(), String> {
        match &event {
            LayerEvent::PointerMotion { surface, x, y }
            | LayerEvent::PointerButton { surface, x, y, .. }
            | LayerEvent::PointerAxis { surface, x, y, .. } => {
                self.pointer = Some((*surface, *x, *y))
            }
            LayerEvent::PointerLeave { surface } => {
                if self.pointer.is_some_and(|(on, _, _)| on == *surface) {
                    self.pointer = None;
                }
            }
            _ => {}
        }
        if let LayerEvent::PointerButton {
            surface,
            pressed: true,
            ..
        } = &event
        {
            self.keyboard = Some(*surface);
        }
        let layouts = Layouts(&self.surfaces);
        match handle_pointer_event(
            &mut self.runtime,
            &mut NoCursor,
            &mut self.input,
            &layouts,
            event,
        )? {
            Ok(_) => {}
            Err(event) => return Err(format!("not a pointer event: {event:?}")),
        }
        self.frame(Duration::ZERO);
        Ok(())
    }

    /// Presses and releases a button at a point of a surface: a click.
    pub(crate) fn click(
        &mut self,
        surface: SurfaceRole,
        (x, y): (f64, f64),
        button: u32,
        modifiers: morf_app::KeyModifiers,
    ) -> Result<(), String> {
        self.pointer(LayerEvent::PointerMotion { surface, x, y })?;
        self.pointer(LayerEvent::PointerButton {
            surface,
            button,
            pressed: true,
            x,
            y,
            modifiers,
        })?;
        self.pointer(LayerEvent::PointerButton {
            surface,
            button,
            pressed: false,
            x,
            y,
            modifiers,
        })
    }

    /// One key, pressed and released, into a surface's focused node.
    pub(crate) fn key(
        &mut self,
        surface: SurfaceRole,
        keysym: u32,
        text: Option<&str>,
        modifiers: morf_lua::KeyModifiers,
    ) -> Result<(), String> {
        self.key_phase(surface, keysym, text, modifiers, true, true)
    }

    /// A key pressed, let go, or both: a modifier held across other keys.
    pub(crate) fn key_phase(
        &mut self,
        surface: SurfaceRole,
        keysym: u32,
        text: Option<&str>,
        modifiers: morf_lua::KeyModifiers,
        press: bool,
        release: bool,
    ) -> Result<(), String> {
        let root = self
            .surfaces
            .iter()
            .find(|candidate| candidate.role == surface)
            .map(|candidate| candidate.root)
            .ok_or_else(|| "no surface to type into".to_owned())?;
        let mut focused = self.input.focused.get(&surface).copied();
        let actions: Vec<KeyAction> = [(press, KeyAction::Press { repeat: false }), (release, KeyAction::Release)]
            .into_iter()
            .filter_map(|(on, action)| on.then_some(action))
            .collect();
        for action in actions {
            dispatch_key_in_subtree(
                &mut self.runtime,
                root,
                &mut focused,
                action,
                keysym,
                text,
                modifiers,
            );
        }
        match focused {
            Some(node) => self.input.focused.insert(surface, node),
            None => self.input.focused.remove(&surface),
        };
        self.frame(Duration::ZERO);
        Ok(())
    }
}

/// The keysym a key name stands for, and the text it types.
///
/// X keysym names, the ones `xkbcommon` uses (`Return`, `Escape`, `Left`,
/// `F5`), a few friendlier spellings (`Enter`, `Esc`), and any single
/// character, whose keysym is its code point the way X assigns them.
pub(crate) fn keysym(name: &str) -> Option<(u32, Option<String>)> {
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
pub(crate) fn modifiers(names: &[String]) -> Result<morf_lua::KeyModifiers, String> {
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
pub(crate) fn pointer_modifiers(names: Option<&str>) -> Result<morf_app::KeyModifiers, String> {
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
pub(crate) fn button(name: &str) -> Result<u32, String> {
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
