// How the person's screens want text smoothed, as fontconfig says.
//
// fontconfig is where a desktop records that a panel's subpixels run red,
// green, blue (`rgba`) and how much their colour fringes are softened
// (`lcdfilter`); every toolkit that draws LCD text reads it there. Asked once
// per process, like the generic families, and not waited on for long.

use std::sync::OnceLock;

/// The order of a pixel's subpixels, as fontconfig's `rgba` names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubpixelOrder {
    /// fontconfig does not say (`unknown`, or no fontconfig at all): the
    /// output's own report decides.
    #[default]
    Unknown,
    Rgb,
    Bgr,
    VerticalRgb,
    VerticalBgr,
    /// Asked for greyscale (`none`).
    None,
}

impl SubpixelOrder {
    /// From fontconfig's integer (`FC_RGBA_*`).
    pub fn from_fontconfig(value: i64) -> Self {
        match value {
            1 => Self::Rgb,
            2 => Self::Bgr,
            3 => Self::VerticalRgb,
            4 => Self::VerticalBgr,
            5 => Self::None,
            _ => Self::Unknown,
        }
    }
}

/// How much the colour fringes are softened, as fontconfig's `lcdfilter`
/// names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LcdFilter {
    /// No filter: each subpixel only its own coverage, the sharpest and the
    /// most coloured.
    None,
    /// FreeType's default five-tap filter.
    #[default]
    Default,
    /// The lighter three-tap one.
    Light,
    /// The old intra-pixel one, taken as `Light`.
    Legacy,
}

impl LcdFilter {
    /// From fontconfig's integer (`FC_LCD_*`).
    pub fn from_fontconfig(value: i64) -> Self {
        match value {
            0 => Self::None,
            2 => Self::Light,
            3 => Self::Legacy,
            _ => Self::Default,
        }
    }
}

/// What fontconfig says about LCD text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FontSubpixel {
    pub order: SubpixelOrder,
    pub filter: LcdFilter,
}

impl FontSubpixel {
    /// Reads `fc-match -f '%{rgba} %{lcdfilter}'`'s answer; either part may
    /// be missing, and a missing part is fontconfig not saying.
    pub fn parse(answer: &str) -> Self {
        let mut parts = answer.split_whitespace();
        let order = parts
            .next()
            .and_then(|value| value.parse().ok())
            .map_or(SubpixelOrder::Unknown, SubpixelOrder::from_fontconfig);
        let filter = parts
            .next()
            .and_then(|value| value.parse().ok())
            .map_or(LcdFilter::Default, LcdFilter::from_fontconfig);
        Self { order, filter }
    }
}

static FONTCONFIG_SUBPIXEL: OnceLock<FontSubpixel> = OnceLock::new();

/// fontconfig's `rgba` and `lcdfilter` for the default face, asked once.
pub fn font_subpixel() -> FontSubpixel {
    *FONTCONFIG_SUBPIXEL.get_or_init(|| {
        crate::fc_match(&["-f", "%{rgba} %{lcdfilter}", "sans-serif"])
            .map(|answer| FontSubpixel::parse(&answer))
            .unwrap_or_default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fontconfig_answers_read_as_an_order_and_a_filter() {
        assert_eq!(
            FontSubpixel::parse("1 1"),
            FontSubpixel {
                order: SubpixelOrder::Rgb,
                filter: LcdFilter::Default
            }
        );
        assert_eq!(
            FontSubpixel::parse("2 2"),
            FontSubpixel {
                order: SubpixelOrder::Bgr,
                filter: LcdFilter::Light
            }
        );
        assert_eq!(FontSubpixel::parse("5 0").order, SubpixelOrder::None);
        assert_eq!(FontSubpixel::parse("3 3").order, SubpixelOrder::VerticalRgb);
        // Nothing said: unknown order, the default filter.
        assert_eq!(FontSubpixel::parse(""), FontSubpixel::default());
        assert_eq!(FontSubpixel::parse("0").order, SubpixelOrder::Unknown);
    }
}
