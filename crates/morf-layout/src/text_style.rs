//! How a run of text is set: its line height, spacing, slant and width.
//!
//! Read once from a node and carried into measurement and painting alike, so
//! the two cannot disagree about what a line of it takes.

use std::sync::Arc;

use morf_scene::{Element, NodeHandle, RichText, Scene, Value};

use crate::helpers::LayoutError;

/// The distance from one baseline to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineHeight {
    /// So many times the font size, the way a stylesheet's bare number is.
    Multiple(f64),
    /// A size in logical pixels, written `"24px"`.
    Pixels(f64),
}

impl LineHeight {
    /// The line height for a font size.
    pub fn pixels(self, size: f64) -> f64 {
        match self {
            Self::Multiple(multiple) => size * multiple,
            Self::Pixels(pixels) => pixels,
        }
    }

    /// Reads a line height from its declarative value.
    pub fn parse(value: &Value) -> Result<Self, String> {
        match value {
            Value::Number(multiple) if multiple.is_finite() && *multiple > 0.0 => {
                Ok(Self::Multiple(*multiple))
            }
            Value::String(text) => text
                .strip_suffix("px")
                .and_then(|pixels| pixels.trim().parse::<f64>().ok())
                .filter(|pixels| pixels.is_finite() && *pixels > 0.0)
                .map(Self::Pixels)
                .ok_or_else(|| format!("`{text}` is not a line height")),
            _ => Err("line_height is a multiple of the font size or a `px` size".to_owned()),
        }
    }
}

impl Default for LineHeight {
    fn default() -> Self {
        Self::Multiple(1.2)
    }
}

/// The slant of a face.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

impl FontStyle {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "normal" => Some(Self::Normal),
            "italic" => Some(Self::Italic),
            "oblique" => Some(Self::Oblique),
            _ => None,
        }
    }
}

/// The width of a face, from the narrowest cut to the widest.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FontStretch {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    #[default]
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

impl FontStretch {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "ultra_condensed" => Some(Self::UltraCondensed),
            "extra_condensed" => Some(Self::ExtraCondensed),
            "condensed" => Some(Self::Condensed),
            "semi_condensed" => Some(Self::SemiCondensed),
            "normal" => Some(Self::Normal),
            "semi_expanded" => Some(Self::SemiExpanded),
            "expanded" => Some(Self::Expanded),
            "extra_expanded" => Some(Self::ExtraExpanded),
            "ultra_expanded" => Some(Self::UltraExpanded),
            _ => None,
        }
    }
}

/// Whether a face's optical size follows the font size.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum OpticalSizing {
    /// A face with an `opsz` axis is set at the font size in pixels, unless
    /// `axes` names `opsz` -- CSS's `font-optical-sizing: auto`.
    #[default]
    Auto,
    /// `opsz` stays at the face's default unless `axes` names it.
    None,
}

impl OpticalSizing {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "auto" => Some(Self::Auto),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// Everything about how text is set besides its family, size and weight.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextStyle {
    pub line_height: LineHeight,
    /// Added between letters, in logical pixels.
    pub letter_spacing: f64,
    /// Added at every space, in logical pixels.
    pub word_spacing: f64,
    pub font_style: FontStyle,
    pub font_stretch: FontStretch,
    /// Styled runs from `spans` or `markup`; when present, its text is what
    /// is set, in place of the node's `text`.
    pub rich: Option<Arc<RichText>>,
    /// The colour of a link run that names none.
    pub link_color: Option<morf_scene::Color>,
    /// A variable font's axis settings, in tag order.
    pub axes: Vec<FontAxis>,
    /// Whether `opsz` follows the size when `axes` does not name it.
    pub optical_sizing: OpticalSizing,
}

/// One variation axis of a variable font set to a value, in the font's own
/// units: `wght` 100 to 900, `FILL` 0 to 1, `opsz` in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontAxis {
    /// The axis's four-letter OpenType tag.
    pub tag: [u8; 4],
    pub value: f32,
}

impl FontAxis {
    /// Reads `{ FILL = 1, wght = 500 }`: four-letter tags to numbers.
    pub fn parse_map(value: &Value) -> Result<Vec<Self>, String> {
        let map = match value {
            Value::Map(map) => map,
            Value::Nil => return Ok(Vec::new()),
            Value::List(list) if list.is_empty() => return Ok(Vec::new()),
            _ => return Err("axes is a table of four-letter tags to numbers".to_owned()),
        };
        let mut axes = Vec::with_capacity(map.len());
        for (name, value) in map {
            let tag = <[u8; 4]>::try_from(name.as_bytes())
                .ok()
                .filter(|tag| tag.iter().all(|byte| (0x20..0x7f).contains(byte)))
                .ok_or_else(|| format!("axes: `{name}` is not a four-letter axis tag"))?;
            let value = match value {
                Value::Number(number) if number.is_finite() => *number as f32,
                _ => return Err(format!("axes: `{name}` must be a number")),
            };
            axes.push(Self { tag, value });
        }
        Ok(axes)
    }
}

/// Markup parsed lately, so a label redrawn every frame is read once.
fn parsed_markup(markup: &str) -> Arc<RichText> {
    thread_local! {
        static PARSED: std::cell::RefCell<std::collections::HashMap<String, Arc<RichText>>> =
            Default::default();
    }
    PARSED.with(|parsed| {
        let mut parsed = parsed.borrow_mut();
        if let Some(rich) = parsed.get(markup) {
            return Arc::clone(rich);
        }
        if parsed.len() >= 256 {
            parsed.clear();
        }
        let rich = Arc::new(RichText::from_markup(markup));
        parsed.insert(markup.to_owned(), Arc::clone(&rich));
        rich
    })
}

/// A Text node's runs: its markup if it has any, else its spans.
fn rich_of(scene: &Scene, node: NodeHandle) -> Result<Option<Arc<RichText>>, LayoutError> {
    if scene.element(node)? != Element::Text {
        return Ok(None);
    }
    let markup = scene.string_value(node, "markup")?;
    if !markup.is_empty() {
        return Ok(Some(parsed_markup(markup)));
    }
    RichText::from_spans(scene.current(node, "spans")?)
        .map(|rich| rich.map(Arc::new))
        .map_err(|message| LayoutError::Scene(format!("Text spans: {message}")))
}

impl TextStyle {
    /// Reads a text node's style.
    pub fn from_scene(scene: &Scene, node: NodeHandle) -> Result<Self, LayoutError> {
        let font_style = scene.string_value(node, "font_style")?;
        let font_stretch = scene.string_value(node, "font_stretch")?;
        Ok(Self {
            line_height: LineHeight::parse(scene.current(node, "line_height")?)
                .map_err(|message| LayoutError::Scene(format!("Text: {message}")))?,
            letter_spacing: scene.number(node, "letter_spacing")?,
            word_spacing: scene.number(node, "word_spacing")?,
            font_style: FontStyle::parse(font_style).ok_or_else(|| {
                LayoutError::Scene(format!(
                    "Text: font_style `{font_style}` is not normal, italic or oblique"
                ))
            })?,
            font_stretch: FontStretch::parse(font_stretch).ok_or_else(|| {
                LayoutError::Scene(format!(
                    "Text: font_stretch `{font_stretch}` is not a width from ultra_condensed to ultra_expanded"
                ))
            })?,
            rich: rich_of(scene, node)?,
            link_color: match scene.element(node)? {
                Element::Text => match scene.current(node, "link_color")? {
                    Value::Color(color) => Some(*color),
                    Value::String(text) => morf_scene::Color::parse(text),
                    _ => None,
                },
                _ => None,
            },
            axes: if scene.has_property(node, "axes")? {
                FontAxis::parse_map(scene.current(node, "axes")?)
                    .map_err(|message| LayoutError::Scene(format!("Text: {message}")))?
            } else {
                Vec::new()
            },
            optical_sizing: if scene.has_property(node, "optical_sizing")? {
                let name = scene.string_value(node, "optical_sizing")?;
                OpticalSizing::parse(name).ok_or_else(|| {
                    LayoutError::Scene(format!(
                        "Text: optical_sizing `{name}` is not auto or none"
                    ))
                })?
            } else {
                OpticalSizing::Auto
            },
        })
    }

    /// The axes a face of this style is set at at `size` pixels, besides
    /// `wght` (the weight, which shaping has already): those `axes` names,
    /// and `opsz` at the size when optical sizing is automatic and `axes`
    /// does not name it. A face without one of them ignores it.
    pub fn variation_axes(&self, size: f32) -> Vec<FontAxis> {
        let mut axes: Vec<FontAxis> = self
            .axes
            .iter()
            .filter(|axis| &axis.tag != b"wght")
            .copied()
            .collect();
        if self.optical_sizing == OpticalSizing::Auto && self.axis(b"opsz").is_none() {
            axes.push(FontAxis {
                tag: *b"opsz",
                value: size,
            });
        }
        axes
    }

    /// The value set for one axis, if any.
    pub fn axis(&self, tag: &[u8; 4]) -> Option<f32> {
        self.axes
            .iter()
            .find(|axis| &axis.tag == tag)
            .map(|axis| axis.value)
    }

    /// The style as a key: the numbers by their bits, so two equal styles
    /// hash alike.
    pub fn key(&self) -> TextStyleKey {
        let (line_kind, line_value) = match self.line_height {
            LineHeight::Multiple(value) => (0, value.to_bits()),
            LineHeight::Pixels(value) => (1, value.to_bits()),
        };
        TextStyleKey {
            line_kind,
            line_value,
            letter_spacing: self.letter_spacing.to_bits(),
            word_spacing: self.word_spacing.to_bits(),
            font_style: self.font_style,
            font_stretch: self.font_stretch,
            rich: self.rich.as_ref().map_or(0, |rich| rich.key),
            link_color: self
                .link_color
                .map(|color| [color.red, color.green, color.blue, color.alpha].map(f32::to_bits)),
            axes: self
                .axes
                .iter()
                .map(|axis| (axis.tag, axis.value.to_bits()))
                .collect(),
            optical_sizing: self.optical_sizing,
        }
    }
}

/// A text style as a hashable key.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TextStyleKey {
    line_kind: u8,
    line_value: u64,
    letter_spacing: u64,
    word_spacing: u64,
    font_style: FontStyle,
    font_stretch: FontStretch,
    rich: u64,
    link_color: Option<[u32; 4]>,
    /// Every axis moves glyphs, so every axis is part of the layout.
    axes: Vec<([u8; 4], u32)>,
    optical_sizing: OpticalSizing,
}
