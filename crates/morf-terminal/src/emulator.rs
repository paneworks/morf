//! The screen a terminal program draws: `alacritty_terminal` underneath.
//!
//! Bytes from the program go in through [`Emulator::feed`]; what the program
//! asked of its terminal comes out as replies to write back (a cursor
//! position report, a colour query) and as [`TerminalEvent`]s (a title, a
//! bell). [`Emulator::screen`] turns the grid into the renderer's
//! [`TerminalScreen`], rebuilding only the lines the parser touched since the
//! last picture and sharing the rest.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, TermDamage, TermMode};
use alacritty_terminal::vte::ansi::{Color as AnsiColor, CursorShape, NamedColor, Processor, Rgb};
use morf_scene::{
    TerminalCell, TerminalCursor, TerminalCursorShape, TerminalLine, TerminalMetrics,
    TerminalScreen, cell_style,
};

use crate::input::{self, KeyModes, Modifiers, MouseAction, MouseButton, MouseModes};

/// The most history a terminal may keep, whatever it asks for.
pub const MAX_SCROLLBACK: usize = 100_000;

/// Something the program did that is not drawing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalEvent {
    /// It named itself (OSC 0 or 2); empty when it went back to no name.
    Title(String),
    /// BEL.
    Bell,
    /// It asked for text to be put on the clipboard (OSC 52).
    Clipboard(String),
}

/// The colours a terminal draws with, sRGB with alpha.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Palette {
    pub foreground: [u8; 4],
    pub background: [u8; 4],
    /// The cursor's colour; `None` takes the colour of the text under it.
    pub cursor: Option<[u8; 4]>,
    /// The colour of the character under a block cursor; `None` takes the
    /// background under it.
    pub cursor_text: Option<[u8; 4]>,
    /// The sixteen ANSI colours: the eight, then their bright forms.
    pub ansi: [[u8; 4]; 16],
}

impl Default for Palette {
    fn default() -> Self {
        const fn rgb(value: u32) -> [u8; 4] {
            [(value >> 16) as u8, (value >> 8) as u8, value as u8, 255]
        }
        Self {
            foreground: rgb(0xd8d8d8),
            background: rgb(0x181818),
            cursor: None,
            cursor_text: None,
            ansi: [
                rgb(0x181818),
                rgb(0xac4242),
                rgb(0x90a959),
                rgb(0xf4bf75),
                rgb(0x6a9fb5),
                rgb(0xaa759f),
                rgb(0x75b5aa),
                rgb(0xd8d8d8),
                rgb(0x6b6b6b),
                rgb(0xc55555),
                rgb(0xaac474),
                rgb(0xfeca88),
                rgb(0x82b8c8),
                rgb(0xc28cb8),
                rgb(0x93d3c3),
                rgb(0xf8f8f8),
            ],
        }
    }
}

/// How a picture of the screen is drawn: its colours, its type, and whether
/// it has the keyboard (which decides whether its cursor is solid).
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenStyle {
    pub palette: Palette,
    pub font_family: String,
    pub font_size: f64,
    pub padding: f64,
    pub metrics: TerminalMetrics,
    pub focused: bool,
}

#[derive(Clone)]
struct Listener(Rc<RefCell<Vec<Event>>>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        self.0.borrow_mut().push(event);
    }
}

#[derive(Clone, Copy)]
struct Size {
    columns: usize,
    rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// A terminal's screen and everything it remembers.
pub struct Emulator {
    term: Term<Listener>,
    parser: Processor,
    events: Rc<RefCell<Vec<Event>>>,
    replies: Vec<u8>,
    notices: Vec<TerminalEvent>,
    title: String,
    /// The last picture's lines, reused where nothing changed.
    lines: Vec<Arc<TerminalLine>>,
    /// What the last picture was drawn with; a different style redraws it all.
    style: Option<ScreenStyle>,
    /// Something was fed, scrolled or resized since the last picture.
    changed: bool,
    cell_pixels: (u16, u16),
}

impl Emulator {
    /// An empty screen of `columns` × `rows` keeping `scrollback` lines of
    /// history (at most [`MAX_SCROLLBACK`]).
    pub fn new(columns: usize, rows: usize, scrollback: usize) -> Self {
        let events = Rc::new(RefCell::new(Vec::new()));
        let config = Config {
            scrolling_history: scrollback.min(MAX_SCROLLBACK),
            ..Config::default()
        };
        let size = Size {
            columns: columns.max(2),
            rows: rows.max(1),
        };
        Self {
            term: Term::new(config, &size, Listener(Rc::clone(&events))),
            parser: Processor::new(),
            events,
            replies: Vec::new(),
            notices: Vec::new(),
            title: String::new(),
            lines: Vec::new(),
            style: None,
            changed: true,
            cell_pixels: (8, 16),
        }
    }

    pub fn columns(&self) -> usize {
        self.term.columns()
    }

    pub fn rows(&self) -> usize {
        self.term.screen_lines()
    }

    /// What the program last called itself.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Reads bytes the program wrote.
    pub fn feed(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.parser.advance(&mut self.term, bytes);
        self.changed = true;
        self.drain_events();
    }

    /// When a synchronized update (mode 2026) the program began must be
    /// shown even though it has not ended it, if one is open.
    pub fn sync_deadline(&self) -> Option<Instant> {
        self.parser.sync_timeout().sync_timeout()
    }

    /// Shows a synchronized update whose time has run out. Returns whether
    /// there was one.
    pub fn expire_sync(&mut self, now: Instant) -> bool {
        if self.sync_deadline().is_none_or(|deadline| deadline > now) {
            return false;
        }
        self.parser.stop_sync(&mut self.term);
        self.changed = true;
        self.drain_events();
        true
    }

    fn drain_events(&mut self) {
        let events = std::mem::take(&mut *self.events.borrow_mut());
        for event in events {
            match event {
                Event::PtyWrite(text) => self.replies.extend_from_slice(text.as_bytes()),
                Event::Title(title) => {
                    self.title = title.clone();
                    self.notices.push(TerminalEvent::Title(title));
                }
                Event::ResetTitle => {
                    self.title.clear();
                    self.notices.push(TerminalEvent::Title(String::new()));
                }
                Event::Bell => self.notices.push(TerminalEvent::Bell),
                Event::ClipboardStore(_, text) => {
                    self.notices.push(TerminalEvent::Clipboard(text));
                }
                Event::ColorRequest(index, format) => {
                    let color = self.color_at(index, &self.current_palette());
                    let reply = format(Rgb {
                        r: color[0],
                        g: color[1],
                        b: color[2],
                    });
                    self.replies.extend_from_slice(reply.as_bytes());
                }
                Event::TextAreaSizeRequest(format) => {
                    let reply = format(WindowSize {
                        num_lines: self.rows() as u16,
                        num_cols: self.columns() as u16,
                        cell_width: self.cell_pixels.0,
                        cell_height: self.cell_pixels.1,
                    });
                    self.replies.extend_from_slice(reply.as_bytes());
                }
                _ => {}
            }
        }
    }

    fn current_palette(&self) -> Palette {
        self.style
            .as_ref()
            .map(|style| style.palette.clone())
            .unwrap_or_default()
    }

    /// What the emulator owes the program: answers to its queries.
    pub fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies)
    }

    /// What happened since the last call.
    pub fn take_events(&mut self) -> Vec<TerminalEvent> {
        std::mem::take(&mut self.notices)
    }

    /// Changes the grid's size; the program is told separately, by the pty.
    pub fn resize(&mut self, columns: usize, rows: usize, cell_pixels: (u16, u16)) {
        self.cell_pixels = cell_pixels;
        let size = Size {
            columns: columns.max(2),
            rows: rows.max(1),
        };
        if size.columns != self.columns() || size.rows != self.rows() {
            self.term.resize(size);
            self.changed = true;
        }
    }

    /// Moves the view `lines` into the history (positive) or back towards
    /// the bottom (negative). Returns whether it moved.
    pub fn scroll(&mut self, lines: i32) -> bool {
        let before = self.term.grid().display_offset();
        self.term.scroll_display(Scroll::Delta(lines));
        let moved = self.term.grid().display_offset() != before;
        self.changed |= moved;
        moved
    }

    /// Back to the bottom of the history, where the program is writing.
    pub fn scroll_to_bottom(&mut self) -> bool {
        if self.term.grid().display_offset() == 0 {
            return false;
        }
        self.term.scroll_display(Scroll::Bottom);
        self.changed = true;
        true
    }

    /// How far into the history the view is.
    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    fn mode(&self) -> TermMode {
        *self.term.mode()
    }

    /// Whether the program is on the alternate screen (a full-screen program).
    pub fn alternate_screen(&self) -> bool {
        self.mode().contains(TermMode::ALT_SCREEN)
    }

    /// Whether the wheel on the alternate screen should become arrow keys.
    pub fn alternate_scroll(&self) -> bool {
        self.mode()
            .contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
    }

    /// Whether the program asked for bracketed paste.
    pub fn bracketed_paste(&self) -> bool {
        self.mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// Whether the program wants to hear when it gains and loses focus.
    pub fn focus_reporting(&self) -> bool {
        self.mode().contains(TermMode::FOCUS_IN_OUT)
    }

    /// Which pointer reports the program asked for.
    pub fn mouse_modes(&self) -> MouseModes {
        let mode = self.mode();
        MouseModes {
            click: mode.contains(TermMode::MOUSE_REPORT_CLICK),
            drag: mode.contains(TermMode::MOUSE_DRAG),
            motion: mode.contains(TermMode::MOUSE_MOTION),
            sgr: mode.contains(TermMode::SGR_MOUSE),
        }
    }

    /// The bytes one key press sends, in the program's current modes.
    pub fn encode_key(
        &self,
        keysym: u32,
        text: Option<&str>,
        modifiers: Modifiers,
    ) -> Option<Vec<u8>> {
        let mode = self.mode();
        input::encode_key(
            keysym,
            text,
            modifiers,
            KeyModes {
                app_cursor: mode.contains(TermMode::APP_CURSOR),
                app_keypad: mode.contains(TermMode::APP_KEYPAD),
            },
        )
    }

    /// The bytes one pointer event sends, if the program asked for it.
    pub fn encode_mouse(
        &self,
        button: MouseButton,
        action: MouseAction,
        column: usize,
        row: usize,
        modifiers: Modifiers,
        held: bool,
    ) -> Option<Vec<u8>> {
        input::encode_mouse(
            button,
            action,
            column.min(self.columns().saturating_sub(1)),
            row.min(self.rows().saturating_sub(1)),
            modifiers,
            held,
            self.mouse_modes(),
        )
    }

    /// The bytes a paste sends.
    pub fn encode_paste(&self, text: &str) -> Vec<u8> {
        input::encode_paste(text, self.bracketed_paste())
    }

    /// The screen as the view shows it, one line per row, trailing blanks
    /// trimmed from each row and blank rows from the end.
    pub fn text(&self) -> String {
        let offset = self.display_offset() as i32;
        let grid = self.term.grid();
        let mut lines = Vec::with_capacity(self.rows());
        for row in 0..self.rows() {
            let line = &grid[Line(row as i32 - offset)];
            let mut text = String::new();
            for column in 0..self.columns() {
                let cell = &line[Column(column)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                text.push(if cell.c == '\0' { ' ' } else { cell.c });
                if let Some(extra) = cell.zerowidth() {
                    text.extend(extra.iter());
                }
            }
            text.truncate(text.trim_end().len());
            lines.push(text);
        }
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines.join("\n")
    }

    /// A new picture of the screen, or `None` when nothing on it changed
    /// since the last one with this style.
    pub fn screen(&mut self, style: &ScreenStyle) -> Option<TerminalScreen> {
        let restyled = self.style.as_ref() != Some(style);
        if !restyled && !self.changed {
            return None;
        }
        let (columns, rows) = (self.columns(), self.rows());
        let full = restyled || self.lines.len() != rows;
        let mut damaged = vec![full; rows];
        match self.term.damage() {
            TermDamage::Full => damaged.iter_mut().for_each(|line| *line = true),
            TermDamage::Partial(lines) => {
                for line in lines {
                    if let Some(slot) = damaged.get_mut(line.line) {
                        *slot = true;
                    }
                }
            }
        }
        self.term.reset_damage();
        self.lines
            .resize_with(rows, || Arc::new(TerminalLine::default()));
        let offset = self.display_offset() as i32;
        for (row, damaged) in damaged.into_iter().enumerate() {
            if !damaged {
                continue;
            }
            let line = self.build_line(Line(row as i32 - offset), columns, &style.palette);
            if *self.lines[row] != line {
                self.lines[row] = Arc::new(line);
            }
        }
        self.style = Some(style.clone());
        self.changed = false;
        Some(TerminalScreen {
            columns,
            rows,
            lines: self.lines.clone(),
            cursor: self.cursor(style),
            background: style.palette.background,
            font_family: style.font_family.clone(),
            font_size: style.font_size,
            padding: style.padding,
            metrics: style.metrics,
        })
    }

    fn build_line(&self, line: Line, columns: usize, palette: &Palette) -> TerminalLine {
        let grid = self.term.grid();
        let row = &grid[line];
        let cells = (0..columns)
            .map(|column| {
                let cell = &row[Column(column)];
                let flags = cell.flags;
                let mut foreground = self.resolve(cell.fg, palette);
                let default_background =
                    matches!(cell.bg, AnsiColor::Named(NamedColor::Background));
                let mut background = self.resolve(cell.bg, palette);
                if flags.contains(Flags::DIM) {
                    foreground = dim(foreground);
                }
                let inverse = flags.contains(Flags::INVERSE);
                if inverse {
                    std::mem::swap(&mut foreground, &mut background);
                }
                if flags.contains(Flags::HIDDEN) {
                    foreground = background;
                }
                // The default background is drawn once, behind the grid,
                // rather than once per cell: a cell that shows it says so
                // with no colour at all.
                if default_background && !inverse {
                    background = [0; 4];
                }
                let mut style = 0;
                for (flag, bit) in [
                    (Flags::BOLD, cell_style::BOLD),
                    (Flags::ITALIC, cell_style::ITALIC),
                    (Flags::UNDERLINE, cell_style::UNDERLINE),
                    (Flags::DOTTED_UNDERLINE, cell_style::UNDERLINE),
                    (Flags::DASHED_UNDERLINE, cell_style::UNDERLINE),
                    (Flags::DOUBLE_UNDERLINE, cell_style::DOUBLE_UNDERLINE),
                    (Flags::UNDERCURL, cell_style::UNDERCURL),
                    (Flags::STRIKEOUT, cell_style::STRIKEOUT),
                    (Flags::WIDE_CHAR, cell_style::WIDE),
                    (Flags::WIDE_CHAR_SPACER, cell_style::SPACER),
                    (Flags::LEADING_WIDE_CHAR_SPACER, cell_style::SPACER),
                ] {
                    if flags.contains(flag) {
                        style |= bit;
                    }
                }
                TerminalCell {
                    character: if cell.c == '\0' || cell.c == '\t' {
                        ' '
                    } else {
                        cell.c
                    },
                    combining: cell
                        .zerowidth()
                        .map(|extra| extra.iter().collect::<String>().into_boxed_str()),
                    foreground,
                    background,
                    style,
                }
            })
            .collect();
        TerminalLine { cells }
    }

    fn cursor(&self, style: &ScreenStyle) -> Option<TerminalCursor> {
        let content = self.term.renderable_content();
        let shape = match content.cursor.shape {
            CursorShape::Hidden => return None,
            _ if !style.focused => TerminalCursorShape::HollowBlock,
            CursorShape::Block => TerminalCursorShape::Block,
            CursorShape::Underline => TerminalCursorShape::Underline,
            CursorShape::Beam => TerminalCursorShape::Beam,
            CursorShape::HollowBlock => TerminalCursorShape::HollowBlock,
        };
        let point = content.cursor.point;
        let row = point.line.0 + content.display_offset as i32;
        if row < 0 || row as usize >= self.rows() {
            return None;
        }
        let row = row as usize;
        let column = point.column.0.min(self.columns().saturating_sub(1));
        let cell = self
            .lines
            .get(row)
            .and_then(|line| line.cells.get(column))
            .cloned()
            .unwrap_or_default();
        let under = if cell.background[3] == 0 {
            style.palette.background
        } else {
            cell.background
        };
        Some(TerminalCursor {
            column,
            row,
            shape,
            color: style.palette.cursor.unwrap_or(cell.foreground),
            text_color: style.palette.cursor_text.unwrap_or(under),
            wide: cell.style & cell_style::WIDE != 0,
        })
    }

    fn resolve(&self, color: AnsiColor, palette: &Palette) -> [u8; 4] {
        match color {
            AnsiColor::Spec(rgb) => [rgb.r, rgb.g, rgb.b, 255],
            AnsiColor::Indexed(index) => self.color_at(usize::from(index), palette),
            AnsiColor::Named(named) => self.color_at(named as usize, palette),
        }
    }

    /// A colour by the emulator's index: 0–255 the palette, then the named
    /// ones (foreground, background, cursor, the dim eight, …). A colour the
    /// program set itself (OSC 4, 10, 11) wins over the configuration's.
    fn color_at(&self, index: usize, palette: &Palette) -> [u8; 4] {
        if let Some(Some(rgb)) =
            (index < alacritty_terminal::term::color::COUNT).then(|| self.term.colors()[index])
        {
            return [rgb.r, rgb.g, rgb.b, 255];
        }
        match index {
            0..=15 => palette.ansi[index],
            16..=231 => {
                let index = index - 16;
                let level = |value: usize| {
                    if value == 0 {
                        0
                    } else {
                        (55 + value * 40) as u8
                    }
                };
                [
                    level(index / 36),
                    level((index / 6) % 6),
                    level(index % 6),
                    255,
                ]
            }
            232..=255 => {
                let gray = (8 + (index - 232) * 10) as u8;
                [gray, gray, gray, 255]
            }
            256 => palette.foreground,
            257 => palette.background,
            258 => palette.cursor.unwrap_or(palette.foreground),
            // The dim eight, then bright and dim foreground.
            259..=266 => dim(palette.ansi[index - 259]),
            267 => palette.foreground,
            268 => dim(palette.foreground),
            _ => palette.foreground,
        }
    }
}

/// A colour at two thirds of its brightness, as dim text is drawn.
fn dim(color: [u8; 4]) -> [u8; 4] {
    let scale = |value: u8| (u16::from(value) * 2 / 3) as u8;
    [scale(color[0]), scale(color[1]), scale(color[2]), color[3]]
}
