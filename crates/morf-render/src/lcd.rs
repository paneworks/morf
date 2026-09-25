//! Subpixel (LCD) text: when it is used, and what it computes.
//!
//! A panel's pixel is three stripes, red, green and blue, and text that
//! knows it can place an edge a third of a pixel at a time. The glyph's
//! distance field is read at each stripe's centre and each colour channel
//! gets its own stripe's coverage (glyph_lcd.wgsl); the blend then mixes each
//! channel with what is beneath by its own coverage, which dual-source
//! blending makes possible in one pass.
//!
//! That mix needs something opaque beneath: over a transparent pixel there is
//! no colour for a fringe to be mixed with, and the compositor would blend a
//! coloured smear over whatever is behind the surface. So a glyph is drawn
//! this way only when every one of these holds, and in greyscale otherwise:
//!
//! - the configuration and the output allow it ([`subpixel_text_for`]): the
//!   switch is not `off`, the order is known and horizontal, and the output
//!   is not rotated or flipped (the stripes would run the other way);
//! - the surface is drawn at a whole-number scale (at a fractional one the
//!   buffer is resampled, and a third of a buffer pixel is no stripe);
//! - the glyph is drawn straight into the surface, not into an offscreen
//!   layer (which holds alpha of its own, and is composited later);
//! - an opaque rectangle of the same surface lies beneath the whole glyph, or
//!   the surface as a whole is declared opaque;
//! - the glyph is only moved, not scaled, rotated or skewed;
//! - it is a plain fill: not mid-morph, and with no outline.

use morf_text::{FontSubpixel, LcdFilter, SubpixelOrder};

/// Subpixel text as a renderer draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubpixelText {
    /// The stripes run blue, green, red rather than red, green, blue.
    pub bgr: bool,
    /// How much the fringes are softened.
    pub filter: LcdFilter,
}

impl SubpixelText {
    /// The width, in pixels, each stripe's reading is smoothed over.
    ///
    /// FreeType renders at three times the width and then filters: a box a
    /// stripe wide, then five taps. Matched by variance to one box, the light
    /// filter is a pixel wide and the default one a ninth more; with no
    /// filter only the stripe itself.
    pub fn spread(self) -> f32 {
        match self.filter {
            LcdFilter::None => 1.0 / 3.0,
            LcdFilter::Light | LcdFilter::Legacy => 1.0,
            LcdFilter::Default => 1.11,
        }
    }
}

/// Whether text on one output is drawn in subpixels, and how.
///
/// `setting` is the configuration's `morf.surface.subpixel_text`: `"off"`,
/// `"auto"` (fontconfig's `rgba`, else what the output reports), or
/// `"rgb"`/`"bgr"` to name the order outright. `output_subpixel` is the
/// output's `wl_output.subpixel` (`horizontal_rgb`, ...), and
/// `output_transform` its transform: any but `normal` turns the stripes.
pub fn subpixel_text_for(
    setting: &str,
    font: FontSubpixel,
    output_subpixel: &str,
    output_transform: &str,
) -> Option<SubpixelText> {
    if output_transform != "normal" {
        return None;
    }
    let bgr = match setting {
        "rgb" => false,
        "bgr" => true,
        "auto" => match font.order {
            SubpixelOrder::Rgb => false,
            SubpixelOrder::Bgr => true,
            SubpixelOrder::Unknown => match output_subpixel {
                "horizontal_rgb" => false,
                "horizontal_bgr" => true,
                _ => return None,
            },
            SubpixelOrder::None | SubpixelOrder::VerticalRgb | SubpixelOrder::VerticalBgr => {
                return None;
            }
        },
        _ => return None,
    };
    Some(SubpixelText {
        bgr,
        filter: font.filter,
    })
}

/// Where the two readings about each stripe's centre fall, in pixels: a
/// twelfth either side, a quarter above and below.
pub const LCD_TAPS: [(f32, f32); 2] = [(-1.0 / 12.0, -0.25), (1.0 / 12.0, 0.25)];

/// The stripes' centres left to right, in pixels from the pixel's centre.
pub fn stripe_centres(bgr: bool) -> [f32; 3] {
    let direction = if bgr { -1.0 } else { 1.0 };
    [-direction / 3.0, 0.0, direction / 3.0]
}

fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The CPU reference of glyph_lcd.wgsl's coverage: each channel's coverage
/// of the glyph whose field `field(dx, dy)` reads, `dx`/`dy` in pixels from
/// the pixel's centre. `edge` is where the field crosses the outline and
/// `ramp` how much it changes across one pixel, as the glyph instance has
/// them.
pub fn stripe_coverage(
    text: SubpixelText,
    edge: f32,
    ramp: f32,
    field: impl Fn(f32, f32) -> f32,
) -> [f32; 3] {
    const FIELD_STEP: f32 = 1.0 / 255.0;
    let feather = (ramp * 0.5 * text.spread()).max(FIELD_STEP);
    stripe_centres(text.bgr).map(|centre| {
        LCD_TAPS
            .iter()
            .map(|(dx, dy)| {
                1.0 - smoothstep(edge - feather, edge + feather, field(centre + dx, *dy))
            })
            .sum::<f32>()
            / LCD_TAPS.len() as f32
    })
}

/// What the dual-source blend leaves: `beneath` with `color` over it, each
/// channel by its own coverage times `alpha`. All straight, in the space the
/// target blends in.
pub fn blend_stripes(
    beneath: [f32; 3],
    color: [f32; 3],
    alpha: f32,
    coverage: [f32; 3],
) -> [f32; 3] {
    std::array::from_fn(|channel| {
        let a = coverage[channel] * alpha;
        color[channel] * a + beneath[channel] * (1.0 - a)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RGB: SubpixelText = SubpixelText {
        bgr: false,
        filter: LcdFilter::Default,
    };

    /// A glyph whose outline is a vertical edge at `x = at` pixels, ink to
    /// the right: the field grows outward, one unit (`ramp`) per pixel, and
    /// crosses `edge` there.
    fn stem(at: f32) -> impl Fn(f32, f32) -> f32 {
        move |dx, _| 0.5 - (dx - at) * 0.1
    }

    fn font(order: SubpixelOrder, filter: LcdFilter) -> FontSubpixel {
        FontSubpixel { order, filter }
    }

    #[test]
    fn fontconfig_decides_then_the_output_and_a_turned_output_never() {
        let rgb = font(SubpixelOrder::Rgb, LcdFilter::Default);
        assert_eq!(
            subpixel_text_for("auto", rgb, "unknown", "normal"),
            Some(RGB)
        );
        // fontconfig wins over the output.
        assert_eq!(
            subpixel_text_for("auto", rgb, "horizontal_bgr", "normal").map(|t| t.bgr),
            Some(false)
        );
        let unknown = font(SubpixelOrder::Unknown, LcdFilter::Light);
        assert_eq!(
            subpixel_text_for("auto", unknown, "horizontal_bgr", "normal"),
            Some(SubpixelText {
                bgr: true,
                filter: LcdFilter::Light
            })
        );
        assert_eq!(
            subpixel_text_for("auto", unknown, "unknown", "normal"),
            None
        );
        assert_eq!(subpixel_text_for("auto", unknown, "none", "normal"), None);
        // Greyscale asked for, or stripes that run down the pixel.
        for order in [
            SubpixelOrder::None,
            SubpixelOrder::VerticalRgb,
            SubpixelOrder::VerticalBgr,
        ] {
            assert_eq!(
                subpixel_text_for(
                    "auto",
                    font(order, LcdFilter::Default),
                    "horizontal_rgb",
                    "normal"
                ),
                None
            );
        }
        // A rotated or flipped output turns the stripes.
        for transform in ["90", "180", "270", "flipped", "flipped_90"] {
            assert_eq!(
                subpixel_text_for("rgb", rgb, "horizontal_rgb", transform),
                None
            );
        }
        assert_eq!(
            subpixel_text_for("off", rgb, "horizontal_rgb", "normal"),
            None
        );
        // Named outright, whatever fontconfig says.
        assert_eq!(
            subpixel_text_for(
                "bgr",
                font(SubpixelOrder::None, LcdFilter::Default),
                "unknown",
                "normal"
            )
            .map(|t| t.bgr),
            Some(true)
        );
    }

    #[test]
    fn far_inside_and_far_outside_are_whole_and_empty_in_every_channel() {
        let inside = stripe_coverage(RGB, 0.5, 0.1, stem(-5.0));
        let outside = stripe_coverage(RGB, 0.5, 0.1, stem(5.0));
        assert_eq!(inside, [1.0; 3]);
        assert_eq!(outside, [0.0; 3]);
    }

    #[test]
    fn an_edge_through_the_pixel_covers_the_stripes_nearest_the_ink_most() {
        // Ink to the right of the pixel's centre: blue, the rightmost stripe
        // in RGB order, is the most covered and red the least.
        let [r, g, b] = stripe_coverage(RGB, 0.5, 0.1, stem(0.0));
        assert!(r < g && g < b, "{r} {g} {b}");
        assert!((g - 0.5).abs() < 1e-5, "green sits on the edge: {g}");
        // BGR mirrors it.
        let bgr = SubpixelText { bgr: true, ..RGB };
        let [r2, g2, b2] = stripe_coverage(bgr, 0.5, 0.1, stem(0.0));
        assert!((r2 - b).abs() < 1e-6 && (b2 - r).abs() < 1e-6 && (g2 - g).abs() < 1e-6);
    }

    #[test]
    fn no_filter_is_sharper_than_the_default_one() {
        let sharp = SubpixelText {
            filter: LcdFilter::None,
            ..RGB
        };
        let [r, _, b] = stripe_coverage(sharp, 0.5, 0.1, stem(0.0));
        let [r2, _, b2] = stripe_coverage(RGB, 0.5, 0.1, stem(0.0));
        assert!(b - r > b2 - r2, "{} against {}", b - r, b2 - r2);
    }

    #[test]
    fn the_stripes_average_to_the_edge_a_greyscale_pixel_would_have() {
        // The mean of the three is the coverage of the whole pixel, within
        // what the smoothing moves it -- which reaches a little past the
        // pixel, so only edges well inside it are held to its area -- and ink
        // on one side of an edge is exactly the space on the other: no
        // stripe adds or loses any.
        for step in 0..=20 {
            let at = -1.0 + step as f32 * 0.1;
            let mean = |at| {
                let [r, g, b] = stripe_coverage(RGB, 0.5, 0.1, stem(at));
                (r + g + b) / 3.0
            };
            let area = (0.5 - at).clamp(0.0, 1.0);
            if at.abs() <= 0.25 {
                assert!(
                    (mean(at) - area).abs() < 0.05,
                    "edge at {at}: {} against {area}",
                    mean(at)
                );
            }
            assert!(
                (mean(at) + mean(-at) - 1.0).abs() < 1e-5,
                "edge at {at}: {} and {}",
                mean(at),
                mean(-at)
            );
        }
    }

    #[test]
    fn the_blend_mixes_each_channel_by_its_own_coverage() {
        let white = [1.0; 3];
        let black = [0.0; 3];
        assert_eq!(
            blend_stripes(white, black, 1.0, [0.0, 0.5, 1.0]),
            [1.0, 0.5, 0.0]
        );
        // Translucent text mixes less.
        assert_eq!(blend_stripes(white, black, 0.5, [1.0; 3]), [0.5; 3]);
        // Full coverage in every channel is the plain colour.
        assert_eq!(
            blend_stripes(white, [0.2, 0.4, 0.6], 1.0, [1.0; 3]),
            [0.2, 0.4, 0.6]
        );
    }
}
