//! A terminal's screen as glyph-pipeline instances.
//!
//! Everything a terminal draws is either a solid rectangle — the background,
//! a run of cells with their own background, the cursor, an underline — or a
//! glyph from the atlas, so it goes through the same pipeline as text, in
//! its command's place in the paint order. Backgrounds and a block cursor
//! lie under the glyphs; decorations and an outline cursor over them.
//!
//! The grid is snapped to device pixels once, at its origin, and every cell
//! is a whole number of device pixels, so a column of `│` or a row of `▀`
//! lands on the same pixels in every cell and joins without seams.

use morf_layout::{Geometry, Transform2D};
use morf_scene::{Color, TerminalCursorShape, TerminalScreen, cell_style};
use morf_text::{CellFace, CellText, RasterGlyph, TextSystem};

use super::glyph_batch::PreparedBand;
use super::glyphs::PreparedGlyph;
use super::textures::glyph_field_uniform;
use crate::DistanceFieldStyle;
use crate::effects::color_array;

fn color(rgba: [u8; 4]) -> Color {
    Color::rgba8(rgba[0], rgba[1], rgba[2], rgba[3])
}

/// Where one terminal command's instances go.
pub(crate) struct TerminalOut<'a> {
    pub(crate) glyphs: &'a mut Vec<PreparedGlyph>,
    /// Under the glyphs: backgrounds and a block cursor.
    pub(crate) under: &'a mut Vec<PreparedBand>,
    /// Over them: decorations and an outline cursor.
    pub(crate) over: &'a mut Vec<PreparedBand>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_terminal(
    text_system: &mut TextSystem,
    screen: &TerminalScreen,
    bounds: Geometry,
    transform: Transform2D,
    color_overlay: Color,
    command_index: usize,
    scale: f32,
    out: TerminalOut<'_>,
) {
    let metrics = screen.metrics;
    if metrics.cell_width <= 0.0 || metrics.cell_height <= 0.0 {
        return;
    }
    let scale_f64 = f64::from(scale.max(f32::EPSILON));
    let band = |rect: Geometry, fill: Color| PreparedBand {
        rect,
        color: fill,
        color_overlay,
        transform,
        command_index,
    };
    if screen.background[3] > 0 {
        out.under.push(band(bounds, color(screen.background)));
    }
    // The grid in device pixels: its origin snapped once, each cell whole.
    let origin_x = ((bounds.x + screen.padding) * scale_f64).round();
    let origin_y = ((bounds.y + screen.padding) * scale_f64).round();
    let cell_w = (metrics.cell_width * scale_f64).round().max(1.0);
    let cell_h = (metrics.cell_height * scale_f64).round().max(1.0);
    // A rectangle of cells, in the logical units bands are given in.
    let cells = |column: usize, row: usize, columns: usize, rows: f64| Geometry {
        x: (origin_x + column as f64 * cell_w) / scale_f64,
        y: (origin_y + row as f64 * cell_h) / scale_f64,
        width: columns as f64 * cell_w / scale_f64,
        height: rows * cell_h / scale_f64,
    };
    // A horizontal line through a row, `from_top` device pixels down, whole
    // pixels thick.
    let stroke = (metrics.stroke * scale_f64).round().max(1.0);
    let line_at = |column: usize, row: usize, columns: usize, from_top: f64| Geometry {
        x: (origin_x + column as f64 * cell_w) / scale_f64,
        y: (origin_y
            + row as f64 * cell_h
            + from_top.round().clamp(0.0, (cell_h - stroke).max(0.0)))
            / scale_f64,
        width: columns as f64 * cell_w / scale_f64,
        height: stroke / scale_f64,
    };
    let baseline = (metrics.baseline * scale_f64).round();
    let underline = baseline + metrics.underline_offset * scale_f64;
    let strikeout = baseline - metrics.strikeout_offset * scale_f64;

    // Runs of cells with a background of their own.
    for (row, line) in screen.lines.iter().enumerate() {
        let mut run: Option<(usize, [u8; 4])> = None;
        for (column, cell) in line.cells.iter().enumerate().chain(std::iter::once((
            line.cells.len(),
            &morf_scene::TerminalCell::default(),
        ))) {
            let background = cell.background;
            match run {
                Some((_, current)) if current == background => {}
                _ => {
                    if let Some((start, fill)) = run.take()
                        && fill[3] > 0
                    {
                        out.under
                            .push(band(cells(start, row, column - start, 1.0), color(fill)));
                    }
                    run = Some((column, background));
                }
            }
        }
    }

    let cursor = screen.cursor;
    if let Some(cursor) = cursor {
        let width = if cursor.wide { 2 } else { 1 };
        let fill = color(cursor.color);
        match cursor.shape {
            TerminalCursorShape::Block => {
                out.under
                    .push(band(cells(cursor.column, cursor.row, width, 1.0), fill));
            }
            TerminalCursorShape::Underline => {
                out.over.push(band(
                    line_at(cursor.column, cursor.row, width, cell_h - stroke * 2.0),
                    fill,
                ));
                out.over.push(band(
                    line_at(cursor.column, cursor.row, width, cell_h - stroke),
                    fill,
                ));
            }
            TerminalCursorShape::Beam => {
                let area = cells(cursor.column, cursor.row, 1, 1.0);
                out.over.push(band(
                    Geometry {
                        width: (stroke * 2.0 / scale_f64).min(area.width),
                        ..area
                    },
                    fill,
                ));
            }
            TerminalCursorShape::HollowBlock => {
                let area = cells(cursor.column, cursor.row, width, 1.0);
                let thin = stroke / scale_f64;
                for edge in [
                    Geometry {
                        height: thin,
                        ..area
                    },
                    Geometry {
                        y: area.y + area.height - thin,
                        height: thin,
                        ..area
                    },
                    Geometry {
                        width: thin,
                        ..area
                    },
                    Geometry {
                        x: area.x + area.width - thin,
                        width: thin,
                        ..area
                    },
                ] {
                    out.over.push(band(edge, fill));
                }
            }
        }
    }

    let field = glyph_field_uniform(DistanceFieldStyle::default(), screen.font_size);
    let ramp =
        morf_text::field_units_per_logical_px(screen.font_size as f32) / scale.max(f32::EPSILON);
    let outline = color_array(Color::rgba8(0, 0, 0, 0));
    let mut raster: Vec<RasterGlyph> = Vec::new();
    let face = CellFace {
        family: &screen.font_family,
        size: screen.font_size,
        metrics,
        scale,
    };
    for (row, line) in screen.lines.iter().enumerate() {
        for (column, cell) in line.cells.iter().enumerate() {
            let style = cell.style;
            let x = origin_x + column as f64 * cell_w;
            let y = origin_y + row as f64 * cell_h;
            let span = if style & cell_style::WIDE != 0 { 2 } else { 1 };
            let decorated = style
                & (cell_style::UNDERLINE
                    | cell_style::DOUBLE_UNDERLINE
                    | cell_style::UNDERCURL
                    | cell_style::STRIKEOUT)
                != 0;
            if decorated && style & cell_style::SPACER == 0 {
                let ink = color(cell.foreground);
                if style & (cell_style::UNDERLINE | cell_style::UNDERCURL) != 0 {
                    out.over
                        .push(band(line_at(column, row, span, underline), ink));
                }
                if style & cell_style::DOUBLE_UNDERLINE != 0 {
                    out.over
                        .push(band(line_at(column, row, span, underline), ink));
                    out.over.push(band(
                        line_at(column, row, span, underline + stroke * 2.0),
                        ink,
                    ));
                }
                if style & cell_style::STRIKEOUT != 0 {
                    out.over.push(band(
                        line_at(column, row, span, strikeout - stroke / 2.0),
                        ink,
                    ));
                }
            }
            if style & cell_style::SPACER != 0 || cell.is_blank() {
                continue;
            }
            let under_block = cursor.is_some_and(|cursor| {
                cursor.shape == TerminalCursorShape::Block
                    && cursor.row == row
                    && cursor.column == column
            });
            let ink = if under_block {
                color(cursor.map_or(cell.foreground, |cursor| cursor.text_color))
            } else {
                color(cell.foreground)
            };
            if ink.alpha <= 0.0 {
                continue;
            }
            raster.clear();
            text_system.terminal_glyphs(
                CellText {
                    character: cell.character,
                    combining: cell.combining.as_deref(),
                    bold: style & cell_style::BOLD != 0,
                    italic: style & cell_style::ITALIC != 0,
                },
                &face,
                (x as f32, y as f32),
                &mut raster,
            );
            for glyph in raster.drain(..) {
                if glyph.width == 0 || glyph.height == 0 {
                    continue;
                }
                out.glyphs.push(PreparedGlyph {
                    glyph,
                    morph: None,
                    morph_progress: 0.0,
                    ramp,
                    color: ink,
                    color_overlay,
                    transform,
                    command_index,
                    field,
                    outline_color: outline,
                });
            }
        }
    }
}
