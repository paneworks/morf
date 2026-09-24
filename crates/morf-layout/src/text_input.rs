//! What a text input shows, and how it is shaped.
//!
//! One place for both, because three passes shape a text input — layout for
//! its size, the runtime for where its caret is, and paint for its glyphs —
//! and the caret is only where the glyphs are if all three shaped the same
//! string with the same options. A password shaped as its letters in one
//! pass and as dots in another would put the caret somewhere in the middle
//! of a dot.

use morf_scene::{NodeHandle, Scene};
use unicode_segmentation::UnicodeSegmentation;

use crate::geometry::{TextElide, TextOptions};
use crate::helpers::{LayoutError, positive, text_alignment};
use crate::text_style::TextStyle;

/// The string a text input shapes, and how its offsets map back to the text.
///
/// The field edits `text`; what reaches the shaper may be something else —
/// dots for a password, the placeholder for an empty field — so every offset
/// crossing between the two goes through here.
#[derive(Clone, Debug, PartialEq)]
pub struct InputDisplay {
    /// What is shaped and drawn.
    pub text: String,
    /// Whether `text` is the placeholder rather than anything typed.
    pub placeholder: bool,
    mask: Option<char>,
}

impl InputDisplay {
    /// What a field holding `text` shows.
    ///
    /// A mask draws one character per grapheme, so a letter built from
    /// several code points is still one dot, as it is one keypress to delete.
    pub fn new(text: &str, placeholder: &str, mask: Option<char>) -> Self {
        if text.is_empty() {
            return Self {
                text: placeholder.to_owned(),
                placeholder: true,
                mask,
            };
        }
        let shown = match mask {
            Some(mask) => std::iter::repeat_n(mask, text.graphemes(true).count()).collect(),
            None => text.to_owned(),
        };
        Self {
            text: shown,
            placeholder: false,
            mask,
        }
    }

    /// The shaped offset of a byte offset in the field's text.
    pub fn to_display(&self, text: &str, byte: usize) -> usize {
        if self.placeholder {
            return 0;
        }
        match self.mask {
            Some(mask) => {
                let before = text
                    .grapheme_indices(true)
                    .take_while(|(start, _)| *start < byte)
                    .count();
                before * mask.len_utf8()
            }
            None => byte.min(self.text.len()),
        }
    }

    /// The byte offset in the field's text of a shaped offset.
    pub fn to_model(&self, text: &str, byte: usize) -> usize {
        if self.placeholder {
            return 0;
        }
        match self.mask {
            Some(mask) => {
                let index = byte / mask.len_utf8().max(1);
                text.grapheme_indices(true)
                    .nth(index)
                    .map_or(text.len(), |(start, _)| start)
            }
            None => byte.min(text.len()),
        }
    }
}

/// Everything shaping a text input takes, read once from the node.
#[derive(Clone, Debug, PartialEq)]
pub struct InputShape {
    /// What is shaped.
    pub display: InputDisplay,
    /// Font family stack.
    pub family: String,
    /// Font size in logical pixels.
    pub size: f64,
    /// The rest of what the shaper is told.
    pub options: TextOptions,
    /// Whether the field holds several lines.
    pub multiline: bool,
}

impl InputShape {
    /// Reads a text input's shaping, at the width it has been given.
    ///
    /// `width` is the resolved width when there is one; before layout there
    /// is only the width the node asked for.
    pub fn read(scene: &Scene, node: NodeHandle, width: Option<f64>) -> Result<Self, LayoutError> {
        let multiline = scene.bool_value(node, "multiline")?;
        let mask = if scene.bool_value(node, "password")? {
            Some(
                scene
                    .string_value(node, "password_char")?
                    .chars()
                    .next()
                    .unwrap_or('•'),
            )
        } else {
            None
        };
        let text = scene.string_value(node, "text")?;
        let display = InputDisplay::new(text, scene.string_value(node, "placeholder")?, mask);
        Ok(Self {
            display,
            family: scene.string_value(node, "font_family")?.to_owned(),
            size: scene.number(node, "font_size")?,
            options: TextOptions {
                width: width.or(positive(scene.number(node, "width")?)),
                // One line scrolls rather than breaks, whatever `wrap` says.
                wrap: multiline && scene.bool_value(node, "wrap")?,
                alignment: text_alignment(scene.string_value(node, "horizontal_alignment")?)?,
                elide: TextElide::None,
                font_weight: scene.number(node, "font_weight")?,
                font_source: match scene.string_value(node, "font_source")? {
                    "" => None,
                    source => Some(source.to_owned()),
                },
                max_lines: 0,
                style: TextStyle::from_scene(scene, node)?,
            },
            multiline,
        })
    }

    /// One line's height at this size and style: the least a field is tall,
    /// with nothing in it.
    pub fn line_height(&self) -> f64 {
        self.options.style.line_height.pixels(self.size.max(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::InputDisplay;

    #[test]
    fn a_mask_is_one_character_per_grapheme() {
        // "e" and a combining acute are two code points, one letter.
        let text = "ae\u{301}b";
        let display = InputDisplay::new(text, "", Some('*'));
        assert_eq!(display.text, "***");
        assert_eq!(display.to_display(text, 0), 0);
        assert_eq!(display.to_display(text, 1), 1);
        assert_eq!(display.to_display(text, 4), 2);
        assert_eq!(display.to_display(text, text.len()), 3);
        assert_eq!(display.to_model(text, 2), 4);
        assert_eq!(display.to_model(text, 3), text.len());
    }

    #[test]
    fn a_multibyte_mask_maps_by_its_own_width() {
        let text = "hey";
        let display = InputDisplay::new(text, "", Some('•'));
        assert_eq!(display.text, "•••");
        assert_eq!(display.to_display(text, 2), 2 * '•'.len_utf8());
        assert_eq!(display.to_model(text, 2 * '•'.len_utf8()), 2);
    }

    #[test]
    fn an_empty_field_shows_its_placeholder_with_the_caret_at_its_start() {
        let display = InputDisplay::new("", "Search", None);
        assert!(display.placeholder);
        assert_eq!(display.text, "Search");
        assert_eq!(display.to_display("", 0), 0);
        assert_eq!(display.to_model("", 4), 0);
    }
}
