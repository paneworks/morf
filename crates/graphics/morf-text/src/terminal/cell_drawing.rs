//! Drawing a joining character from the cell's geometry: the coverage
//! canvas and the character-by-character recipes.

use super::box_arms::{arms, draw_arms};

/// Coverage of one cell, drawn from its geometry: a line runs to the cell's
/// edge, so it meets the next cell's on the same pixel.
pub(super) struct Canvas {
    pub(super) width: u32,
    pub(super) height: u32,
    /// Coverage, zero to one, per pixel.
    pixels: Vec<f32>,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0.0; (width * height) as usize],
        }
    }

    /// Fills whole pixels in `[x0, x1) × [y0, y1)`, clamped to the cell.
    pub(super) fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, alpha: f32) {
        let clamp_x = |value: i32| value.clamp(0, self.width as i32) as u32;
        let clamp_y = |value: i32| value.clamp(0, self.height as i32) as u32;
        for y in clamp_y(y0)..clamp_y(y1) {
            for x in clamp_x(x0)..clamp_x(x1) {
                let pixel = &mut self.pixels[(y * self.width + x) as usize];
                *pixel = pixel.max(alpha);
            }
        }
    }

    /// Fills where `inside` holds, sampled four by four per pixel so a curve
    /// or a slope has a soft edge.
    fn shape(&mut self, inside: impl Fn(f32, f32) -> bool) {
        const SAMPLES: u32 = 4;
        for y in 0..self.height {
            for x in 0..self.width {
                let mut hits = 0;
                for sy in 0..SAMPLES {
                    for sx in 0..SAMPLES {
                        let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                        let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                        if inside(px, py) {
                            hits += 1;
                        }
                    }
                }
                if hits > 0 {
                    let pixel = &mut self.pixels[(y * self.width + x) as usize];
                    *pixel = pixel.max(hits as f32 / (SAMPLES * SAMPLES) as f32);
                }
            }
        }
    }

    fn finish(self) -> Vec<u8> {
        self.pixels
            .into_iter()
            .map(|coverage| (coverage.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect()
    }
}

/// Draws one of the characters [`drawn_here`] names, `width` × `height`
/// device pixels, as coverage.
pub(super) fn draw_cell(character: char, width: u32, height: u32, scale: f32) -> Option<Vec<u8>> {
    let mut canvas = Canvas::new(width, height);
    let (w, h) = (width as i32, height as i32);
    // A light line is a device pixel per logical one, at least one; a heavy
    // one twice that.
    let light = (scale.round() as i32).max(1).min(w.max(1));
    let heavy = (light * 2).min(w.max(1));
    // The band of a line of `thickness` centred in `extent`.
    let band = |extent: i32, thickness: i32| {
        let start = (extent - thickness) / 2;
        (start, start + thickness)
    };
    let code = character as u32;
    match code {
        0x2500..=0x257f => {
            if let Some(arms) = arms(character) {
                draw_arms(&mut canvas, arms, light, heavy);
            } else {
                match code {
                    // Dashes: three, four or two segments, light or heavy.
                    0x2504..=0x250b | 0x254c..=0x254f => {
                        let (count, horizontal, heavy_line) = match code {
                            0x2504 => (3, true, false),
                            0x2505 => (3, true, true),
                            0x2506 => (3, false, false),
                            0x2507 => (3, false, true),
                            0x2508 => (4, true, false),
                            0x2509 => (4, true, true),
                            0x250a => (4, false, false),
                            0x250b => (4, false, true),
                            0x254c => (2, true, false),
                            0x254d => (2, true, true),
                            0x254e => (2, false, false),
                            _ => (2, false, true),
                        };
                        let thickness = if heavy_line { heavy } else { light };
                        let extent = if horizontal { w } else { h };
                        let (b0, b1) = band(if horizontal { h } else { w }, thickness);
                        for index in 0..count {
                            let start = extent * index / count;
                            let end = extent * (index + 1) / count;
                            let gap = ((end - start) / 4).max(1);
                            let (s0, s1) = (start + gap / 2, end - (gap - gap / 2));
                            if horizontal {
                                canvas.rect(s0, b0, s1, b1, 1.0);
                            } else {
                                canvas.rect(b0, s0, b1, s1, 1.0);
                            }
                        }
                    }
                    // Rounded corners: a quarter circle joining the two
                    // centre lines, and straight on to the edges.
                    0x256d..=0x2570 => {
                        let (x0, x1) = band(w, light);
                        let (y0, y1) = band(h, light);
                        let cx = (x0 + x1) as f32 / 2.0;
                        let cy = (y0 + y1) as f32 / 2.0;
                        // To the nearer edge, less half a pixel, so the
                        // arc ends inside the cell and a straight piece
                        // carries the line the rest of the way.
                        let radius =
                            (cx.min(w as f32 - cx).min(cy).min(h as f32 - cy) - 0.5).max(1.0);
                        let half = light as f32 / 2.0;
                        // Which way the arms go: right or left, down or up.
                        let (right, down) = match code {
                            0x256d => (true, true),
                            0x256e => (false, true),
                            0x256f => (false, false),
                            _ => (true, false),
                        };
                        let centre_x = if right { cx + radius } else { cx - radius };
                        let centre_y = if down { cy + radius } else { cy - radius };
                        canvas.shape(|px, py| {
                            let in_quadrant =
                                (if right {
                                    px <= centre_x
                                } else {
                                    px >= centre_x
                                }) && (if down { py <= centre_y } else { py >= centre_y });
                            in_quadrant && {
                                let distance =
                                    ((px - centre_x).powi(2) + (py - centre_y).powi(2)).sqrt();
                                (distance - radius).abs() <= half
                            }
                        });
                        let arc_x = centre_x.floor() as i32;
                        let arc_y = centre_y.floor() as i32;
                        if right {
                            canvas.rect(arc_x, y0, w, y1, 1.0);
                        } else {
                            canvas.rect(0, y0, arc_x + 1, y1, 1.0);
                        }
                        if down {
                            canvas.rect(x0, arc_y, x1, h, 1.0);
                        } else {
                            canvas.rect(x0, 0, x1, arc_y + 1, 1.0);
                        }
                    }
                    // Diagonals.
                    0x2571..=0x2573 => {
                        let half = light as f32 / 2.0 * 1.2;
                        let (wf, hf) = (w as f32, h as f32);
                        let length = (wf * wf + hf * hf).sqrt();
                        let rising = move |px: f32, py: f32| {
                            ((hf * px + wf * py - wf * hf) / length).abs() <= half
                        };
                        let falling =
                            move |px: f32, py: f32| ((hf * px - wf * py) / length).abs() <= half;
                        match code {
                            0x2571 => canvas.shape(rising),
                            0x2572 => canvas.shape(falling),
                            _ => canvas.shape(|px, py| rising(px, py) || falling(px, py)),
                        }
                    }
                    _ => return None,
                }
            }
        }
        // Block elements.
        0x2580..=0x259f => {
            let eighth_h = |n: i32| (h * n + 4) / 8;
            let eighth_w = |n: i32| (w * n + 4) / 8;
            let (half_w, half_h) = (w / 2, h / 2);
            match code {
                0x2580 => canvas.rect(0, 0, w, half_h, 1.0),
                0x2581..=0x2588 => {
                    let n = (code - 0x2580) as i32;
                    canvas.rect(0, h - eighth_h(n), w, h, 1.0);
                }
                0x2589..=0x258f => {
                    let n = 8 - (code - 0x2588) as i32;
                    canvas.rect(0, 0, eighth_w(n), h, 1.0);
                }
                0x2590 => canvas.rect(half_w, 0, w, h, 1.0),
                0x2591 => canvas.rect(0, 0, w, h, 0.25),
                0x2592 => canvas.rect(0, 0, w, h, 0.5),
                0x2593 => canvas.rect(0, 0, w, h, 0.75),
                0x2594 => canvas.rect(0, 0, w, eighth_h(1), 1.0),
                0x2595 => canvas.rect(w - eighth_w(1), 0, w, h, 1.0),
                _ => {
                    // Quadrants: upper left, upper right, lower left, lower right.
                    let quadrants: [bool; 4] = match code {
                        0x2596 => [false, false, true, false],
                        0x2597 => [false, false, false, true],
                        0x2598 => [true, false, false, false],
                        0x2599 => [true, false, true, true],
                        0x259a => [true, false, false, true],
                        0x259b => [true, true, true, false],
                        0x259c => [true, true, false, true],
                        0x259d => [false, true, false, false],
                        0x259e => [false, true, true, false],
                        _ => [false, true, true, true],
                    };
                    let boxes = [
                        (0, 0, half_w, half_h),
                        (half_w, 0, w, half_h),
                        (0, half_h, half_w, h),
                        (half_w, half_h, w, h),
                    ];
                    for (on, (x0, y0, x1, y1)) in quadrants.into_iter().zip(boxes) {
                        if on {
                            canvas.rect(x0, y0, x1, y1, 1.0);
                        }
                    }
                }
            }
        }
        // Braille: two columns of four dots, round, spread over the cell.
        0x2800..=0x28ff => {
            let bits = code - 0x2800;
            if bits == 0 {
                return Some(canvas.finish());
            }
            // Dot n's column and row, as the pattern numbers them.
            const DOTS: [(u32, u32); 8] = [
                (0, 0),
                (0, 1),
                (0, 2),
                (1, 0),
                (1, 1),
                (1, 2),
                (0, 3),
                (1, 3),
            ];
            let (wf, hf) = (w as f32, h as f32);
            let radius = (wf / 4.0).min(hf / 8.0) * 0.8;
            let centres: Vec<(f32, f32)> = DOTS
                .iter()
                .enumerate()
                .filter(|(index, _)| bits & (1 << index) != 0)
                .map(|(_, (column, row))| {
                    (
                        wf * (1.0 + 2.0 * *column as f32) / 4.0,
                        hf * (1.0 + 2.0 * *row as f32) / 8.0,
                    )
                })
                .collect();
            canvas.shape(|px, py| {
                centres
                    .iter()
                    .any(|(cx, cy)| (px - cx).powi(2) + (py - cy).powi(2) <= radius * radius)
            });
        }
        // Powerline: a solid triangle or a chevron, pointing right or left.
        0xe0b0..=0xe0b3 => {
            let (wf, hf) = (w as f32, h as f32);
            let half = light as f32 / 2.0 * 1.4;
            match code {
                0xe0b0 => canvas.shape(|px, py| px / wf <= 1.0 - (2.0 * py / hf - 1.0).abs()),
                0xe0b2 => {
                    canvas.shape(|px, py| (wf - px) / wf <= 1.0 - (2.0 * py / hf - 1.0).abs())
                }
                0xe0b1 | 0xe0b3 => {
                    let left = code == 0xe0b3;
                    canvas.shape(|px, py| {
                        let x = if left { wf - px } else { px };
                        // The two strokes from the corners to the middle.
                        let along = if py <= hf / 2.0 { py } else { hf - py };
                        let target = along / (hf / 2.0) * wf;
                        (x - target).abs() <= half * (1.0 + (wf / hf) * 2.0).min(3.0)
                    });
                }
                _ => return None,
            }
        }
        _ => return None,
    }
    Some(canvas.finish())
}
