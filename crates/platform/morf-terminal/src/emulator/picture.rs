//! Turning the grid into the renderer's picture: lines, the cursor, and
//! colours resolved through the palette.

use std::sync::Arc;

use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermDamage;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, CursorShape, NamedColor};
use morf_scene::{
    TerminalCell, TerminalCursor, TerminalCursorShape, TerminalLine, TerminalScreen, cell_style,
};

use super::{Emulator, Palette, ScreenStyle};

impl Emulator {
    /// A new picture of the screen, or `None` when nothing on it changed
    /// since the last one with this style.
    pub fn screen(&mut self, style: &ScreenStyle) -> Option<TerminalScreen> {
        let restyled = self.style.as_ref() != Some(style);
        if !restyled && !self.changed {
            return None;
        }
        let (columns, rows) = (self.columns(), self.rows());
        let full =
            restyled || self.lines.len() != rows || std::mem::take(&mut self.selection_moved);
        let selected = self
            .term
            .selection
            .as_ref()
            .and_then(|selection| selection.to_range(&self.term));
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
            let line = self.build_line(
                Line(row as i32 - offset),
                columns,
                &style.palette,
                selected.as_ref(),
            );
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

    fn build_line(
        &self,
        line: Line,
        columns: usize,
        palette: &Palette,
        selected: Option<&SelectionRange>,
    ) -> TerminalLine {
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
                // A selected cell is drawn inverted, as an inverse one is
                // drawn plain.
                let chosen =
                    selected.is_some_and(|range| range.contains(Point::new(line, Column(column))));
                let inverse = flags.contains(Flags::INVERSE) != chosen;
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
    pub(super) fn color_at(&self, index: usize, palette: &Palette) -> [u8; 4] {
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
