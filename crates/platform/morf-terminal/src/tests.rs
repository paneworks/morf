use std::sync::Arc;
use std::time::{Duration, Instant};

use morf_io::{IoEvent, Reactor};
use morf_scene::{TerminalCursorShape, TerminalMetrics, cell_style};

use crate::input::{self, KeyModes, MouseModes};
use crate::*;

fn style() -> ScreenStyle {
    ScreenStyle {
        palette: Palette::default(),
        font_family: "monospace".into(),
        font_size: 13.0,
        padding: 0.0,
        metrics: TerminalMetrics::default(),
        focused: true,
    }
}

#[test]
fn text_lands_on_the_grid() {
    let mut emulator = Emulator::new(20, 4, 100);
    emulator.feed(b"hello\r\nworld");
    assert_eq!(emulator.text(), "hello\nworld");
    let screen = emulator.screen(&style()).expect("a first picture");
    assert_eq!((screen.columns, screen.rows), (20, 4));
    assert_eq!(screen.lines[0].text(), "hello");
    assert_eq!(screen.text(), "hello\nworld");
    let cursor = screen.cursor.expect("a visible cursor");
    assert_eq!((cursor.column, cursor.row), (5, 1));
    assert_eq!(cursor.shape, TerminalCursorShape::Block);
}

#[test]
fn cursor_movement_and_erasing() {
    let mut emulator = Emulator::new(10, 3, 0);
    emulator.feed(b"abcdef\x1b[1;3HX\x1b[2;1Hsecond\x1b[1;5H\x1b[K");
    assert_eq!(emulator.text(), "abXd\nsecond");
    emulator.feed(b"\x1b[2J\x1b[H");
    assert_eq!(emulator.text(), "");
}

#[test]
fn colours_are_resolved() {
    let mut emulator = Emulator::new(10, 2, 0);
    // Red on the default, a 256-colour blue, truecolour, and inverse.
    emulator.feed(b"\x1b[31mr\x1b[38;5;21mb\x1b[38;2;1;2;3mt\x1b[0;7mi");
    let palette = Palette::default();
    let screen = emulator.screen(&style()).unwrap();
    let cells = &screen.lines[0].cells;
    assert_eq!(cells[0].foreground, palette.ansi[1]);
    assert_eq!(
        cells[0].background,
        [0, 0, 0, 0],
        "the default background is left to the grid"
    );
    assert_eq!(cells[1].foreground, [0, 0, 255, 255]);
    assert_eq!(cells[2].foreground, [1, 2, 3, 255]);
    assert_eq!(cells[3].foreground, palette.background);
    assert_eq!(cells[3].background, palette.foreground);
}

#[test]
fn styles_become_bits() {
    let mut emulator = Emulator::new(10, 1, 0);
    emulator.feed(b"\x1b[1mb\x1b[0;3mi\x1b[0;4mu\x1b[0;9ms");
    let screen = emulator.screen(&style()).unwrap();
    let styles: Vec<u8> = screen.lines[0].cells[..4]
        .iter()
        .map(|cell| cell.style)
        .collect();
    assert_eq!(
        styles,
        [
            cell_style::BOLD,
            cell_style::ITALIC,
            cell_style::UNDERLINE,
            cell_style::STRIKEOUT
        ]
    );
}

#[test]
fn wide_characters_take_two_cells() {
    let mut emulator = Emulator::new(10, 1, 0);
    emulator.feed("日x".as_bytes());
    let screen = emulator.screen(&style()).unwrap();
    let cells = &screen.lines[0].cells;
    assert_eq!(cells[0].character, '日');
    assert_ne!(cells[0].style & cell_style::WIDE, 0);
    assert_ne!(cells[1].style & cell_style::SPACER, 0);
    assert_eq!(cells[2].character, 'x');
    assert_eq!(emulator.text(), "日x");
}

#[test]
fn box_drawing_and_braille_are_single_cells() {
    let mut emulator = Emulator::new(10, 1, 0);
    emulator.feed("╭─┬─╮⣿⡇".as_bytes());
    let screen = emulator.screen(&style()).unwrap();
    let text: String = screen.lines[0].cells[..7]
        .iter()
        .map(|cell| cell.character)
        .collect();
    assert_eq!(text, "╭─┬─╮⣿⡇");
}

#[test]
fn an_unchanged_screen_is_not_redrawn_and_untouched_lines_are_shared() {
    let mut emulator = Emulator::new(10, 3, 0);
    emulator.feed(b"one\r\ntwo\r\nthree");
    let first = emulator.screen(&style()).unwrap();
    assert!(
        emulator.screen(&style()).is_none(),
        "nothing fed, nothing new"
    );
    emulator.feed(b"\x1b[1;1HONE");
    let second = emulator.screen(&style()).unwrap();
    assert_eq!(second.lines[0].text(), "ONE");
    assert!(
        Arc::ptr_eq(&first.lines[1], &second.lines[1]),
        "line two was not touched"
    );
    // A different style redraws everything.
    let mut unfocused = style();
    unfocused.focused = false;
    let third = emulator.screen(&unfocused).unwrap();
    assert_eq!(
        third.cursor.unwrap().shape,
        TerminalCursorShape::HollowBlock
    );
}

#[test]
fn the_alternate_screen_comes_and_goes() {
    let mut emulator = Emulator::new(10, 2, 10);
    emulator.feed(b"shell");
    emulator.feed(b"\x1b[?1049h\x1b[Hfull");
    assert!(emulator.alternate_screen());
    assert_eq!(emulator.text(), "full");
    emulator.feed(b"\x1b[?1049l");
    assert!(!emulator.alternate_screen());
    assert_eq!(emulator.text(), "shell");
}

#[test]
fn a_title_a_bell_and_a_query_are_answered() {
    let mut emulator = Emulator::new(10, 5, 0);
    emulator.feed(b"\x1b]0;my title\x07\x07ab\x1b[6n");
    assert_eq!(emulator.title(), "my title");
    assert_eq!(
        emulator.take_events(),
        [TerminalEvent::Title("my title".into()), TerminalEvent::Bell]
    );
    // Cursor position report: row 1, column 3, one-based.
    assert_eq!(emulator.take_replies(), b"\x1b[1;3R");
}

#[test]
fn history_scrolls() {
    let mut emulator = Emulator::new(10, 2, 100);
    emulator.feed(b"1\r\n2\r\n3\r\n4");
    assert_eq!(emulator.text(), "3\n4");
    assert!(emulator.scroll(2));
    assert_eq!(emulator.text(), "1\n2");
    assert!(emulator.scroll_to_bottom());
    assert_eq!(emulator.text(), "3\n4");
}

#[test]
fn resizing_rewraps_nothing_it_should_not() {
    let mut emulator = Emulator::new(10, 2, 0);
    emulator.feed(b"abc");
    emulator.resize(40, 5, (8, 16));
    assert_eq!((emulator.columns(), emulator.rows()), (40, 5));
    let screen = emulator.screen(&style()).unwrap();
    assert_eq!(screen.lines.len(), 5);
    assert_eq!(screen.text(), "abc");
}

#[test]
fn keys_are_encoded_as_xterm_does() {
    let none = Modifiers::default();
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::default()
    };
    let alt = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    let normal = KeyModes::default();
    let app = KeyModes {
        app_cursor: true,
        ..KeyModes::default()
    };
    let key = |sym, text, modifiers, modes| input::encode_key(sym, text, modifiers, modes);
    assert_eq!(key(0xff52, None, none, normal).unwrap(), b"\x1b[A");
    assert_eq!(key(0xff52, None, none, app).unwrap(), b"\x1bOA");
    assert_eq!(key(0xff53, None, ctrl, app).unwrap(), b"\x1b[1;5C");
    assert_eq!(key(0xff50, None, none, normal).unwrap(), b"\x1b[H");
    assert_eq!(key(0xff57, None, none, normal).unwrap(), b"\x1b[F");
    assert_eq!(key(0xff55, None, none, normal).unwrap(), b"\x1b[5~");
    assert_eq!(key(0xff56, None, shift, normal).unwrap(), b"\x1b[6;2~");
    assert_eq!(key(0xffff, None, none, normal).unwrap(), b"\x1b[3~");
    assert_eq!(key(0xffbe, None, none, normal).unwrap(), b"\x1bOP");
    assert_eq!(key(0xffc2, None, none, normal).unwrap(), b"\x1b[15~");
    assert_eq!(key(0xffc9, None, none, normal).unwrap(), b"\x1b[24~");
    assert_eq!(key(0xffc9, None, ctrl, normal).unwrap(), b"\x1b[24;5~");
    assert_eq!(key(0xff0d, Some("\r"), none, normal).unwrap(), b"\r");
    assert_eq!(key(0xff08, None, none, normal).unwrap(), b"\x7f");
    assert_eq!(key(0xff09, Some("\t"), none, normal).unwrap(), b"\t");
    assert_eq!(key(0xfe20, None, shift, normal).unwrap(), b"\x1b[Z");
    assert_eq!(key(0xff1b, None, none, normal).unwrap(), b"\x1b");
    assert_eq!(key(0x63, Some("\x03"), ctrl, normal).unwrap(), b"\x03");
    assert_eq!(key(0x20, Some(" "), ctrl, normal).unwrap(), b"\x00");
    assert_eq!(key(0x62, Some("b"), alt, normal).unwrap(), b"\x1bb");
    assert_eq!(key(0x61, Some("a"), none, normal).unwrap(), b"a");
    assert_eq!(key(0xe9, Some("é"), none, normal).unwrap(), "é".as_bytes());
    // A bare modifier sends nothing.
    assert_eq!(key(0xffe3, None, ctrl, normal), None);
}

#[test]
fn the_pointer_is_reported_only_when_asked_for() {
    let none = Modifiers::default();
    let off = MouseModes::default();
    let sgr = MouseModes {
        click: true,
        sgr: true,
        ..MouseModes::default()
    };
    let press = |modes| {
        input::encode_mouse(
            MouseButton::Left,
            MouseAction::Press,
            4,
            2,
            none,
            false,
            modes,
        )
    };
    assert_eq!(press(off), None);
    assert_eq!(press(sgr).unwrap(), b"\x1b[<0;5;3M");
    assert_eq!(
        input::encode_mouse(
            MouseButton::Left,
            MouseAction::Release,
            4,
            2,
            none,
            false,
            sgr
        )
        .unwrap(),
        b"\x1b[<0;5;3m"
    );
    assert_eq!(
        input::encode_mouse(
            MouseButton::WheelDown,
            MouseAction::Press,
            0,
            0,
            none,
            false,
            sgr
        )
        .unwrap(),
        b"\x1b[<65;1;1M"
    );
    // Motion only under 1002 with a button held, or under 1003.
    assert_eq!(
        input::encode_mouse(
            MouseButton::None,
            MouseAction::Motion,
            0,
            0,
            none,
            false,
            sgr
        ),
        None
    );
    let drag = MouseModes { drag: true, ..sgr };
    assert_eq!(
        input::encode_mouse(
            MouseButton::Left,
            MouseAction::Motion,
            1,
            1,
            none,
            true,
            drag
        )
        .unwrap(),
        b"\x1b[<32;2;2M"
    );
    // The X10 form, for a program that did not ask for SGR.
    let x10 = MouseModes {
        click: true,
        ..MouseModes::default()
    };
    assert_eq!(press(x10).unwrap(), [0x1b, b'[', b'M', 32, 37, 35]);
}

#[test]
fn a_paste_is_bracketed_when_asked_for() {
    assert_eq!(input::encode_paste("a\nb", false), b"a\rb");
    assert_eq!(
        input::encode_paste("a\x1b[201~b", true),
        b"\x1b[200~ab\x1b[201~"
    );
    let mut emulator = Emulator::new(10, 2, 0);
    emulator.feed(b"\x1b[?2004h");
    assert_eq!(emulator.encode_paste("x"), b"\x1b[200~x\x1b[201~");
}

/// Runs a command on a pty until it exits, feeding an emulator.
fn run_on_pty(
    command: &[&str],
    size: PtySize,
    input: Option<(Duration, &[u8], PtySize)>,
) -> (Emulator, Option<i32>) {
    let reactor = Reactor::new().unwrap();
    let mut pty = Pty::spawn(
        &reactor,
        &PtyOptions {
            command: command.iter().map(|word| word.to_string()).collect(),
            size,
            ..PtyOptions::default()
        },
    )
    .unwrap();
    let mut emulator = Emulator::new(usize::from(size.columns), usize::from(size.rows), 0);
    let deadline = Instant::now() + Duration::from_secs(10);
    let started = Instant::now();
    let mut input = input;
    loop {
        if let Some((after, bytes, resize)) = input
            && started.elapsed() >= after
        {
            pty.resize(resize).unwrap();
            emulator.resize(
                usize::from(resize.columns),
                usize::from(resize.rows),
                (8, 16),
            );
            pty.write(bytes.to_vec()).unwrap();
            input = None;
        }
        assert!(
            Instant::now() < deadline,
            "the command did not finish: {}",
            emulator.text()
        );
        let Some(event) = reactor.next_timeout(Duration::from_millis(20)) else {
            continue;
        };
        pty.credit(event.weight());
        match event {
            IoEvent::Stdout(_, bytes) => {
                emulator.feed(&bytes);
                let replies = emulator.take_replies();
                if !replies.is_empty() {
                    pty.write(replies).unwrap();
                }
            }
            IoEvent::Exit { code, .. } => {
                pty.close();
                return (emulator, code);
            }
            _ => {}
        }
    }
}

#[test]
fn a_program_on_a_pty_is_heard_and_its_exit_reported() {
    let size = PtySize {
        columns: 40,
        rows: 5,
        cell_width: 8,
        cell_height: 16,
    };
    let (emulator, code) = run_on_pty(
        &[
            "sh",
            "-c",
            "printf '\\033[32mgreen\\033[0m %s\\n' \"$TERM\"; exit 3",
        ],
        size,
        None,
    );
    assert_eq!(code, Some(3));
    assert_eq!(emulator.text(), "green xterm-256color");
}

#[test]
fn a_program_on_a_pty_has_a_terminal_of_its_own() {
    let size = PtySize {
        columns: 57,
        rows: 11,
        cell_width: 8,
        cell_height: 16,
    };
    let (emulator, code) = run_on_pty(
        &[
            "sh",
            "-c",
            "stty size; tty -s && echo tty; test -z \"$LD_LIBRARY_PATH\" && echo clean",
        ],
        size,
        None,
    );
    assert_eq!(code, Some(0));
    assert_eq!(emulator.text(), "11 57\ntty\nclean");
}

#[test]
fn a_resize_reaches_the_program() {
    let size = PtySize {
        columns: 80,
        rows: 24,
        cell_width: 8,
        cell_height: 16,
    };
    let bigger = PtySize {
        columns: 100,
        rows: 30,
        ..size
    };
    let (emulator, code) = run_on_pty(
        &[
            "sh",
            "-c",
            "read line; stty size; tput cols 2>/dev/null || echo 100",
        ],
        size,
        Some((Duration::from_millis(200), b"go\r", bigger)),
    );
    assert_eq!(code, Some(0));
    assert!(
        emulator.text().ends_with("30 100\n100"),
        "{}",
        emulator.text()
    );
}
