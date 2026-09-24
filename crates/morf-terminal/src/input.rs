//! Keys and the pointer, as the bytes a terminal program reads.
//!
//! The encodings are xterm's, which is what `TERM=xterm-256color` promises:
//! cursor keys in normal or application mode, the `CSI 1;m X` and
//! `CSI n;m ~` forms with a modifier parameter, Alt as an Escape prefix,
//! Ctrl folding a letter to its control code, and SGR (1006) mouse reports
//! with the legacy X10 form for programs that did not ask for SGR.

/// Modifiers held with a key or a pointer event.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub logo: bool,
}

impl Modifiers {
    /// xterm's modifier parameter: one more than the bit sum, one meaning
    /// nothing held.
    fn parameter(self) -> u8 {
        1 + u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.ctrl)
    }

    fn any(self) -> bool {
        self.ctrl || self.shift || self.alt
    }
}

/// The modes of the program that decide how a key is encoded.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KeyModes {
    /// `DECCKM`: cursor keys send `SS3 A` rather than `CSI A`.
    pub app_cursor: bool,
    /// `DECKPAM`: the keypad sends its own sequences.
    pub app_keypad: bool,
}

mod keysym {
    pub const BACKSPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;
    pub const LINEFEED: u32 = 0xff0a;
    pub const RETURN: u32 = 0xff0d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const HOME: u32 = 0xff50;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
    pub const PAGE_UP: u32 = 0xff55;
    pub const PAGE_DOWN: u32 = 0xff56;
    pub const END: u32 = 0xff57;
    pub const BEGIN: u32 = 0xff58;
    pub const INSERT: u32 = 0xff63;
    pub const DELETE: u32 = 0xffff;
    pub const ISO_LEFT_TAB: u32 = 0xfe20;
    pub const KP_ENTER: u32 = 0xff8d;
    pub const KP_HOME: u32 = 0xff95;
    pub const KP_LEFT: u32 = 0xff96;
    pub const KP_UP: u32 = 0xff97;
    pub const KP_RIGHT: u32 = 0xff98;
    pub const KP_DOWN: u32 = 0xff99;
    pub const KP_PAGE_UP: u32 = 0xff9a;
    pub const KP_PAGE_DOWN: u32 = 0xff9b;
    pub const KP_END: u32 = 0xff9c;
    pub const KP_BEGIN: u32 = 0xff9d;
    pub const KP_INSERT: u32 = 0xff9e;
    pub const KP_DELETE: u32 = 0xff9f;
    pub const F1: u32 = 0xffbe;
    pub const F35: u32 = 0xffe0;
}

/// Encodes one key press, or `None` for a key a terminal has nothing to
/// send for (a bare modifier, a media key).
///
/// `text` is what the keyboard layout made of the key, used for anything
/// that is not a named key.
pub fn encode_key(
    sym: u32,
    text: Option<&str>,
    modifiers: Modifiers,
    modes: KeyModes,
) -> Option<Vec<u8>> {
    use keysym::*;
    let alt_prefix = |mut bytes: Vec<u8>| {
        if modifiers.alt {
            bytes.insert(0, 0x1b);
        }
        bytes
    };
    // Cursor keys: `CSI A`, `SS3 A` in application mode, `CSI 1;m A` held.
    let cursor = |letter: u8| {
        if modifiers.any() {
            format!("\x1b[1;{}{}", modifiers.parameter(), letter as char).into_bytes()
        } else if modes.app_cursor {
            vec![0x1b, b'O', letter]
        } else {
            vec![0x1b, b'[', letter]
        }
    };
    // Editing keys: `CSI n ~`, `CSI n;m ~` held.
    let tilde = |number: u8| {
        if modifiers.any() {
            format!("\x1b[{number};{}~", modifiers.parameter()).into_bytes()
        } else {
            format!("\x1b[{number}~").into_bytes()
        }
    };
    let bytes = match sym {
        UP | KP_UP => cursor(b'A'),
        DOWN | KP_DOWN => cursor(b'B'),
        RIGHT | KP_RIGHT => cursor(b'C'),
        LEFT | KP_LEFT => cursor(b'D'),
        HOME | KP_HOME => cursor(b'H'),
        END | KP_END => cursor(b'F'),
        BEGIN | KP_BEGIN => cursor(b'E'),
        INSERT | KP_INSERT => tilde(2),
        DELETE | KP_DELETE => tilde(3),
        PAGE_UP | KP_PAGE_UP => tilde(5),
        PAGE_DOWN | KP_PAGE_DOWN => tilde(6),
        F1..=F35 => {
            let index = sym - F1;
            // F1–F4 are `SS3 P`–`SS3 S`, or `CSI 1;m P` held.
            if index < 4 {
                let letter = b"PQRS"[index as usize];
                if modifiers.any() {
                    format!("\x1b[1;{}{}", modifiers.parameter(), letter as char).into_bytes()
                } else {
                    vec![0x1b, b'O', letter]
                }
            } else {
                // F5 onwards skip the numbers the VT220 kept for others.
                const CODES: [u8; 20] = [
                    15, 17, 18, 19, 20, 21, 23, 24, 25, 26, 28, 29, 31, 32, 33, 34, 42, 43, 44, 45,
                ];
                tilde(*CODES.get(index as usize - 4)?)
            }
        }
        RETURN | LINEFEED => alt_prefix(vec![b'\r']),
        KP_ENTER if modes.app_keypad && !modifiers.any() => b"\x1bOM".to_vec(),
        KP_ENTER => alt_prefix(vec![b'\r']),
        BACKSPACE => {
            // DEL, as every modern terminal and `stty erase` agree; Ctrl
            // makes it the older ^H, which some programs read as a word.
            alt_prefix(vec![if modifiers.ctrl { 0x08 } else { 0x7f }])
        }
        ISO_LEFT_TAB => b"\x1b[Z".to_vec(),
        TAB if modifiers.shift => b"\x1b[Z".to_vec(),
        TAB => alt_prefix(vec![b'\t']),
        ESCAPE => alt_prefix(vec![0x1b]),
        _ => {
            if modifiers.ctrl
                && let Some(code) = control_code(sym)
            {
                return Some(alt_prefix(vec![code]));
            }
            let text = text.filter(|text| !text.is_empty())?;
            // A layout's control character with Ctrl held is already what
            // the program wants; anything else it made is typed as it is.
            alt_prefix(text.as_bytes().to_vec())
        }
    };
    Some(bytes)
}

/// The control code Ctrl makes of a key, by its keysym: letters, and the
/// punctuation xterm folds the same way (`^@` is Ctrl+Space or Ctrl+2).
fn control_code(sym: u32) -> Option<u8> {
    let character = char::from_u32(sym)?;
    Some(match character {
        'a'..='z' => character as u8 - b'a' + 1,
        'A'..='Z' => character as u8 - b'A' + 1,
        ' ' | '@' | '2' | '`' => 0,
        '[' | '3' | '{' => 0x1b,
        '\\' | '4' | '|' => 0x1c,
        ']' | '5' | '}' => 0x1d,
        '^' | '6' | '~' => 0x1e,
        '_' | '7' | '/' | '-' => 0x1f,
        '8' | '?' => 0x7f,
        _ => return None,
    })
}

/// A pointer button, as a terminal numbers it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    WheelLeft,
    WheelRight,
    /// Motion with nothing held.
    None,
}

impl MouseButton {
    /// The button for a Linux input code.
    pub fn from_code(code: u32) -> Option<Self> {
        Some(match code {
            0x110 => Self::Left,
            0x111 => Self::Right,
            0x112 => Self::Middle,
            _ => return None,
        })
    }

    fn number(self) -> u32 {
        match self {
            Self::Left => 0,
            Self::Middle => 1,
            Self::Right => 2,
            Self::None => 3,
            Self::WheelUp => 64,
            Self::WheelDown => 65,
            Self::WheelLeft => 66,
            Self::WheelRight => 67,
        }
    }
}

/// What the pointer did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseAction {
    Press,
    Release,
    Motion,
}

/// Which pointer reports the program asked for.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MouseModes {
    /// 1000: presses and releases.
    pub click: bool,
    /// 1002: motion while a button is held.
    pub drag: bool,
    /// 1003: all motion.
    pub motion: bool,
    /// 1006: the SGR encoding.
    pub sgr: bool,
}

impl MouseModes {
    /// Whether the program wants the pointer at all.
    pub fn any(self) -> bool {
        self.click || self.drag || self.motion
    }
}

/// Encodes one pointer event at a cell (zero-based), or `None` when the
/// program did not ask for this kind of event.
///
/// `held` is whether a button is down, for motion under mode 1002.
pub fn encode_mouse(
    button: MouseButton,
    action: MouseAction,
    column: usize,
    row: usize,
    modifiers: Modifiers,
    held: bool,
    modes: MouseModes,
) -> Option<Vec<u8>> {
    if !modes.any() {
        return None;
    }
    let wheel = matches!(
        button,
        MouseButton::WheelUp
            | MouseButton::WheelDown
            | MouseButton::WheelLeft
            | MouseButton::WheelRight
    );
    if action == MouseAction::Motion && !(modes.motion || (modes.drag && held)) {
        return None;
    }
    if wheel && action == MouseAction::Release {
        return None;
    }
    let mut code = button.number();
    if action == MouseAction::Motion {
        code += 32;
    }
    code += 4 * u32::from(modifiers.shift)
        + 8 * u32::from(modifiers.alt)
        + 16 * u32::from(modifiers.ctrl);
    let (x, y) = (column + 1, row + 1);
    if modes.sgr {
        let end = if action == MouseAction::Release {
            'm'
        } else {
            'M'
        };
        return Some(format!("\x1b[<{code};{x};{y}{end}").into_bytes());
    }
    // X10: a release does not say which button, and each number is one
    // byte offset by 32, so nothing past column 223 can be said at all.
    if action == MouseAction::Release {
        code = 3 + (code & !3);
    }
    if x > 223 || y > 223 {
        return None;
    }
    Some(vec![
        0x1b,
        b'[',
        b'M',
        32 + code as u8,
        32 + x as u8,
        32 + y as u8,
    ])
}

/// Text to paste, as the program should receive it: inside the bracketed
/// paste markers when it asked for them (and with any markers in the text
/// itself taken out, so a paste cannot end its own bracket early), and with
/// newlines as the carriage returns a keyboard's Enter sends otherwise.
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let clean = text.replace("\x1b[201~", "").replace("\x1b[200~", "");
        let mut bytes = b"\x1b[200~".to_vec();
        bytes.extend_from_slice(clean.as_bytes());
        bytes.extend_from_slice(b"\x1b[201~");
        bytes
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}
