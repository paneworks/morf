//! Input for a headless configuration: the shell's own pointer and key
//! paths, fed with events nothing real produced.
//!
//! Split from `headless` at the line gate.

use std::time::Duration;

use morf_layout::Layout;
use morf_wayland::{LayerEvent, SurfaceRole};

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
struct Layouts<'a>(&'a [Surface]);

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
            _ => {}
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
    ) -> Result<(), String> {
        self.pointer(LayerEvent::PointerMotion { surface, x, y })?;
        self.pointer(LayerEvent::PointerButton {
            surface,
            button,
            pressed: true,
            x,
            y,
        })?;
        self.pointer(LayerEvent::PointerButton {
            surface,
            button,
            pressed: false,
            x,
            y,
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
        let root = self
            .surfaces
            .iter()
            .find(|candidate| candidate.role == surface)
            .map(|candidate| candidate.root)
            .ok_or_else(|| "no surface to type into".to_owned())?;
        let mut focused = self.input.focused.get(&surface).copied();
        for action in [KeyAction::Press { repeat: false }, KeyAction::Release] {
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
    let named = match name {
        "Return" | "Enter" | "return" | "enter" => Some((0xff0d, Some("\r"))),
        "Escape" | "Esc" | "escape" | "esc" => Some((0xff1b, None)),
        "Tab" | "tab" => Some((0xff09, Some("\t"))),
        "ISO_Left_Tab" => Some((0xfe20, None)),
        "BackSpace" | "Backspace" | "backspace" => Some((0xff08, None)),
        "Delete" | "delete" => Some((0xffff, None)),
        "Insert" | "insert" => Some((0xff63, None)),
        "Home" | "home" => Some((0xff50, None)),
        "End" | "end" => Some((0xff57, None)),
        "Left" | "left" => Some((0xff51, None)),
        "Up" | "up" => Some((0xff52, None)),
        "Right" | "right" => Some((0xff53, None)),
        "Down" | "down" => Some((0xff54, None)),
        "Page_Up" | "PageUp" | "pageup" => Some((0xff55, None)),
        "Page_Down" | "PageDown" | "pagedown" => Some((0xff56, None)),
        "space" | "Space" => Some((0x20, Some(" "))),
        _ => None,
    };
    if let Some((keysym, text)) = named {
        return Some((keysym, text.map(str::to_owned)));
    }
    if let Some(number) = name
        .strip_prefix('F')
        .and_then(|rest| rest.parse::<u32>().ok())
        .filter(|number| (1..=35).contains(number))
    {
        return Some((0xffbe + number - 1, None));
    }
    let mut chars = name.chars();
    let (Some(only), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let code = only as u32;
    let keysym = if (0x20..=0x7e).contains(&code) || (0xa0..=0xff).contains(&code) {
        code
    } else {
        0x0100_0000 + code
    };
    Some((keysym, Some(only.to_string())))
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
