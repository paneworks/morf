//! How big one of morf's pixels is.
//!
//! A compositor's scale is a guess (Hyprland's `auto` makes a 14" 1080p panel
//! 1.5, a 32" 4K one 1.0, and the first comes out a third larger to the eye),
//! so morf can count in its own unit instead: asked for, one
//! [`REFERENCE_PPI`](crate::density::REFERENCE_PPI)th of an inch on every panel that says how big it is,
//! like Android's dp, or a fixed scale. By default it takes the
//! compositor's. Everything above the window system -- layout, text,
//! Lua, an app -- counts in that unit; the window system converts at its
//! edge with a [`Zoom`](crate::density::Zoom).

/// How many of morf's pixels make an inch when measured by the panel.
/// A 32" 4K panel (139 per inch) is one device pixel to one of morf's.
pub const REFERENCE_PPI: f64 = 140.0;

/// Below this many pixels per inch a panel is a television or a projector,
/// looked at from across a room, and its millimetres say nothing about how
/// big things should be on it.
const NEAR_PPI: f64 = 80.0;
/// Above this the millimetres are made up.
const MAX_PPI: f64 = 600.0;
/// A scale this close to the compositor's is the compositor's: an integer
/// scale keeps text on whole pixels, which is worth a few percent of size.
const SNAP: f64 = 0.06;
/// Scales go in steps of 1/24, so a panel a millimetre off in its report
/// does not get a scale of its own.
const STEP_120: u32 = 5;

/// What decides the scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Density {
    /// The compositor's scale, as it is.
    Compositor,
    /// One of morf's pixels is this many per inch, on a panel that says how
    /// big it is; the compositor's scale on one that does not.
    Ppi(f64),
    /// This many device pixels to one of morf's, everywhere.
    Scale(f64),
    /// The compositor's scale times this (a scale slider: 0.5 half the
    /// size, 2 twice it, 1 the compositor's own).
    Relative(f64),
}

/// The compositor's scale: a phone's 2 or 3 is right for it, and a
/// compositor told to scale a desk panel by 1 draws morf at 1. Measuring by
/// the panel, or a fixed scale, is asked for.
impl Default for Density {
    fn default() -> Self {
        Self::Compositor
    }
}

/// Pixels per inch of a panel `pixels` wide and high (its mode, as it is
/// shown) that is `millimetres` big; `None` when that is not believable:
/// nothing reported, a television, or millimetres whose shape is not the
/// pixels' shape (rotated panels reported unrotated are turned back).
pub fn ppi(pixels: (u32, u32), millimetres: (u32, u32)) -> Option<f64> {
    let (width, height) = (f64::from(pixels.0), f64::from(pixels.1));
    let (mut wide, mut high) = (f64::from(millimetres.0), f64::from(millimetres.1));
    if width < 1.0 || height < 1.0 || wide < 1.0 || high < 1.0 {
        return None;
    }
    if (width > height) != (wide > high) {
        std::mem::swap(&mut wide, &mut high);
    }
    let across = (width / wide * 25.4, height / high * 25.4);
    if (across.0 - across.1).abs() > across.0.max(across.1) * 0.15 {
        return None;
    }
    let ppi = (across.0 + across.1) / 2.0;
    (NEAR_PPI..=MAX_PPI).contains(&ppi).then_some(ppi)
}

impl Density {
    /// The scale, in 120ths, for a window on a panel of `ppi` the
    /// compositor scales by `compositor_120`.
    pub fn scale_120(self, ppi: Option<f64>, compositor_120: u32) -> u32 {
        let compositor_120 = compositor_120.max(1);
        let wanted = match self {
            Self::Compositor => return compositor_120,
            Self::Ppi(reference) => match ppi {
                Some(ppi) if reference > 0.0 => ppi / reference,
                _ => return compositor_120,
            },
            Self::Scale(scale) if scale > 0.0 => scale,
            Self::Scale(_) => return compositor_120,
            // Every step of a slider counts, so no snapping back to the
            // compositor's but at the very middle.
            Self::Relative(factor) if factor > 0.0 => {
                if (factor - 1.0).abs() < 1e-3 {
                    return compositor_120;
                }
                let scale = f64::from(compositor_120) / 120.0 * factor;
                return ((scale * 120.0).round() as u32).clamp(30, 1200);
            }
            Self::Relative(_) => return compositor_120,
        };
        let compositor = f64::from(compositor_120) / 120.0;
        if (wanted - compositor).abs() <= compositor * SNAP {
            return compositor_120;
        }
        let steps = (wanted * 120.0 / f64::from(STEP_120)).round().max(1.0);
        (steps as u32 * STEP_120).clamp(30, 1200)
    }
}

/// An output `pixels` big in device pixels, `logical` in the compositor's
/// logical ones and `millimetres` big if it says: its size in morf's pixels,
/// and the scale (120ths) morf draws on it at.
pub fn output_units(
    density: Density,
    pixels: (u32, u32),
    logical: (u32, u32),
    millimetres: Option<(u32, u32)>,
) -> ((u32, u32), u32) {
    if logical.0 == 0 || logical.1 == 0 {
        return (logical, 120);
    }
    let logical_120 = (u64::from(pixels.0) * 120 / u64::from(logical.0)).max(1) as u32;
    let units = density.scale_120(millimetres.and_then(|mm| ppi(pixels, mm)), logical_120);
    let zoom = Zoom::new(logical_120, units);
    ((zoom.size_in(logical.0), zoom.size_in(logical.1)), units)
}

/// The conversion between the compositor's logical pixels and morf's at one
/// window: `units` device pixels to one of morf's, `logical` to one of the
/// compositor's, both in 120ths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zoom {
    pub logical: u32,
    pub units: u32,
}

impl Zoom {
    pub fn new(logical_120: u32, units_120: u32) -> Self {
        Self {
            logical: logical_120.max(1),
            units: units_120.max(1),
        }
    }

    /// Whether this converts nothing.
    pub fn is_identity(self) -> bool {
        self.logical == self.units
    }

    /// The compositor's logical pixels per one of morf's.
    fn factor(self) -> f64 {
        f64::from(self.units) / f64::from(self.logical)
    }

    /// A position from the compositor, in morf's pixels.
    pub fn point_in(self, value: f64) -> f64 {
        value / self.factor()
    }

    /// A position in morf's pixels, for the compositor.
    pub fn point_out(self, value: f64) -> f64 {
        value * self.factor()
    }

    /// A size the compositor gave, in morf's pixels: what covers it, short
    /// of it by less than half a device pixel rather than a whole one over.
    pub fn size_in(self, value: u32) -> u32 {
        if self.is_identity() {
            return value;
        }
        let exact = f64::from(value) / self.factor();
        let floor = exact.floor();
        let short = (exact - floor) * f64::from(self.units) / 120.0;
        if short < 0.5 {
            floor as u32
        } else {
            floor as u32 + 1
        }
    }

    /// A size in morf's pixels, for the compositor; zero stays zero (it
    /// means "the compositor picks").
    pub fn size_out(self, value: u32) -> u32 {
        if self.is_identity() || value == 0 {
            return value;
        }
        (f64::from(value) * self.factor()).round().max(1.0) as u32
    }

    /// A signed length (a margin, an offset, an exclusive zone) for the
    /// compositor; negative ones keep their meaning.
    pub fn length_out(self, value: i32) -> i32 {
        if self.is_identity() {
            return value;
        }
        (f64::from(value) * self.factor()).round() as i32
    }

    /// A rectangle in morf's pixels, for the compositor: the smallest one
    /// in its pixels that holds it.
    pub fn rect_out(self, x: i32, y: i32, width: u32, height: u32) -> (i32, i32, u32, u32) {
        if self.is_identity() {
            return (x, y, width, height);
        }
        let left = self.point_out(f64::from(x)).floor();
        let top = self.point_out(f64::from(y)).floor();
        let right = self.point_out(f64::from(x) + f64::from(width)).ceil();
        let bottom = self.point_out(f64::from(y) + f64::from(height)).ceil();
        (
            left as i32,
            top as i32,
            (right - left).max(0.0) as u32,
            (bottom - top).max(0.0) as u32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panels_measure_by_what_they_say() {
        // The desk: a 32" 4K panel.
        let desk = ppi((3840, 2160), (700, 400)).unwrap();
        assert!((desk - 138.9).abs() < 1.0, "{desk}");
        // A T480s: 14" 1080p.
        let laptop = ppi((1920, 1080), (310, 170)).unwrap();
        assert!((laptop - 158.0).abs() < 2.0, "{laptop}");
        // Turned on its side, reported unturned.
        assert!(ppi((1080, 1920), (310, 170)).is_some());
        assert_eq!(ppi((1920, 1080), (0, 0)), None);
        // A 65" television.
        assert_eq!(ppi((3840, 2160), (1430, 800)), None);
        // An EDID whose millimetres are an aspect ratio.
        assert_eq!(ppi((1920, 1080), (16, 9)), None);
        assert_eq!(ppi((1920, 1080), (400, 400)), None);
    }

    #[test]
    fn a_pixel_is_the_same_size_on_every_panel() {
        // By default the compositor's.
        assert_eq!(Density::default().scale_120(Some(300.0), 180), 180);
        let density = Density::Ppi(REFERENCE_PPI);
        // The desk keeps its scale of one.
        assert_eq!(density.scale_120(ppi((3840, 2160), (700, 400)), 120), 120);
        // The laptop Hyprland scales by 1.5 gets 1.125.
        assert_eq!(density.scale_120(ppi((1920, 1080), (310, 170)), 180), 135);
        // A panel that says nothing keeps the compositor's.
        assert_eq!(density.scale_120(None, 180), 180);
        assert_eq!(Density::Compositor.scale_120(Some(300.0), 120), 120);
        assert_eq!(Density::Scale(2.0).scale_120(None, 120), 240);
        assert_eq!(Density::Ppi(160.0).scale_120(Some(160.0), 150), 120);
        // A slider's: the compositor's times its factor, every step.
        assert_eq!(Density::Relative(1.0).scale_120(None, 180), 180);
        assert_eq!(Density::Relative(0.5).scale_120(None, 180), 90);
        assert_eq!(Density::Relative(1.05).scale_120(None, 120), 126);
    }

    #[test]
    fn sizes_go_out_and_come_back() {
        // The laptop's output, as a configuration sees it.
        assert_eq!(
            output_units(
                Density::Ppi(REFERENCE_PPI),
                (1920, 1080),
                (1280, 720),
                Some((310, 170))
            ),
            ((1707, 960), 135)
        );
        let zoom = Zoom::new(180, 135);
        // A 1280-logical-pixel-wide output is 1707 of morf's pixels.
        assert_eq!(zoom.size_in(1280), 1707);
        for asked in 1..400 {
            let back = zoom.size_in(zoom.size_out(asked));
            assert!(back.abs_diff(asked) <= 1, "{asked} came back {back}");
        }
        assert_eq!(zoom.size_out(0), 0);
        assert_eq!(zoom.length_out(-1), -1);
        assert_eq!(zoom.length_out(40), 30);
        assert!((zoom.point_in(zoom.point_out(12.5)) - 12.5).abs() < 1e-9);
        assert_eq!(zoom.rect_out(1, 1, 2, 2), (0, 0, 3, 3));
        let same = Zoom::new(120, 120);
        assert_eq!(same.size_in(1001), 1001);
        assert_eq!(same.rect_out(1, 2, 3, 4), (1, 2, 3, 4));
    }
}
