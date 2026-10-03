//! Keys by name: the X keysym names `xkbcommon` uses (`Return`, `Escape`,
//! `Left`, `F5`), for the handlers a key reaches and for `morf.keys`.
//!
//! A handler gets the keysym as a number and its name beside it, so a
//! configuration compares `name == "Down"` (or `keysym == morf.keys.Down`)
//! instead of carrying a table of numbers. A printable key's name is its
//! character, as X names them.

/// Every named key: its X name and keysym.
pub const NAMED: &[(&str, u32)] = &[
    ("Return", 0xff0d),
    ("Escape", 0xff1b),
    ("Tab", 0xff09),
    ("ISO_Left_Tab", 0xfe20),
    ("BackSpace", 0xff08),
    ("Delete", 0xffff),
    ("Insert", 0xff63),
    ("Home", 0xff50),
    ("End", 0xff57),
    ("Left", 0xff51),
    ("Up", 0xff52),
    ("Right", 0xff53),
    ("Down", 0xff54),
    ("Page_Up", 0xff55),
    ("Page_Down", 0xff56),
    ("space", 0x20),
    ("KP_Enter", 0xff8d),
    ("Menu", 0xff67),
    ("Print", 0xff61),
    ("Pause", 0xff13),
    ("Scroll_Lock", 0xff14),
    ("Caps_Lock", 0xffe5),
    ("Num_Lock", 0xff7f),
    ("Shift_L", 0xffe1),
    ("Shift_R", 0xffe2),
    ("Control_L", 0xffe3),
    ("Control_R", 0xffe4),
    ("Alt_L", 0xffe9),
    ("Alt_R", 0xffea),
    ("Super_L", 0xffeb),
    ("Super_R", 0xffec),
    ("XF86AudioRaiseVolume", 0x1008ff13),
    ("XF86AudioLowerVolume", 0x1008ff11),
    ("XF86AudioMute", 0x1008ff12),
    ("XF86AudioPlay", 0x1008ff14),
    ("XF86AudioPause", 0x1008ff31),
    ("XF86AudioNext", 0x1008ff17),
    ("XF86AudioPrev", 0x1008ff16),
    ("XF86MonBrightnessUp", 0x1008ff02),
    ("XF86MonBrightnessDown", 0x1008ff03),
    // What a mouse's back and forward buttons press, as shortcuts see them.
    ("XF86Back", 0x1008ff26),
    ("XF86Forward", 0x1008ff27),
];

/// Friendlier spellings a name may be given as.
const ALIASES: &[(&str, &str)] = &[
    ("Enter", "Return"),
    ("enter", "Return"),
    ("return", "Return"),
    ("Esc", "Escape"),
    ("esc", "Escape"),
    ("escape", "Escape"),
    ("tab", "Tab"),
    ("Backspace", "BackSpace"),
    ("backspace", "BackSpace"),
    ("delete", "Delete"),
    ("insert", "Insert"),
    ("home", "Home"),
    ("end", "End"),
    ("left", "Left"),
    ("up", "Up"),
    ("right", "Right"),
    ("down", "Down"),
    ("PageUp", "Page_Up"),
    ("pageup", "Page_Up"),
    ("PageDown", "Page_Down"),
    ("pagedown", "Page_Down"),
    ("back", "XF86Back"),
    ("forward", "XF86Forward"),
    ("Space", "space"),
];

/// A key's keysym by name (an X name, an alias, `F1`..`F35`, or one
/// character).
pub fn keysym(name: &str) -> Option<u32> {
    let canonical = ALIASES
        .iter()
        .find(|(alias, _)| *alias == name)
        .map_or(name, |(_, to)| to);
    if let Some((_, keysym)) = NAMED.iter().find(|(n, _)| *n == canonical) {
        return Some(*keysym);
    }
    if let Some(number) = name
        .strip_prefix('F')
        .and_then(|rest| rest.parse::<u32>().ok())
        .filter(|number| (1..=35).contains(number))
    {
        return Some(0xffbe + number - 1);
    }
    let mut chars = name.chars();
    let (Some(only), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let code = only as u32;
    Some(
        if (0x20..=0x7e).contains(&code) || (0xa0..=0xff).contains(&code) {
            code
        } else {
            0x0100_0000 + code
        },
    )
}

/// A keysym's name: its X name, `F1`..`F35`, or its character.
pub fn name(keysym: u32) -> Option<String> {
    if let Some((name, _)) = NAMED.iter().find(|(_, k)| *k == keysym) {
        return Some((*name).to_owned());
    }
    if (0xffbe..=0xffbe + 34).contains(&keysym) {
        return Some(format!("F{}", keysym - 0xffbe + 1));
    }
    let code = if (0x21..=0x7e).contains(&keysym) || (0xa0..=0xff).contains(&keysym) {
        keysym
    } else if (0x0100_0000..=0x0110_ffff).contains(&keysym) {
        keysym - 0x0100_0000
    } else {
        return None;
    };
    char::from_u32(code).map(|c| c.to_string())
}

/// `morf.keys`: every named key's keysym by its X name (`morf.keys.Down`),
/// for comparing against the number a handler gets.
pub(crate) fn install_keys_api<'gc>(ctx: luna::Context<'gc>, morf: luna::Table<'gc>) {
    let keys = luna::Table::new(&ctx);
    for (name, keysym) in NAMED {
        keys.set_field(ctx, name, i64::from(*keysym));
    }
    for number in 1..=35u32 {
        let name = format!("F{number}");
        let _ = keys.set(
            ctx,
            ctx.intern(name.as_bytes()),
            i64::from(0xffbe + number - 1),
        );
    }
    morf.set_field(ctx, "keys", keys);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_keysyms_go_both_ways() {
        for (name, keysym) in NAMED {
            assert_eq!(super::keysym(name), Some(*keysym), "{name}");
            assert_eq!(super::name(*keysym).as_deref(), Some(*name), "{keysym:#x}");
        }
        assert_eq!(keysym("Enter"), Some(0xff0d));
        assert_eq!(keysym("down"), Some(0xff54));
        assert_eq!(keysym("F5"), Some(0xffc2));
        assert_eq!(name(0xffc2).as_deref(), Some("F5"));
        assert_eq!(keysym("a"), Some(0x61));
        assert_eq!(name(0x61).as_deref(), Some("a"));
        assert_eq!(keysym("é"), Some(0xe9));
        assert_eq!(keysym("€"), Some(0x0100_20ac));
        assert_eq!(name(0x0100_20ac).as_deref(), Some("€"));
        assert_eq!(keysym("nonsense"), None);
        assert_eq!(name(0xfe03), None);
    }
}
