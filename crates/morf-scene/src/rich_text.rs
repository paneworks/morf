//! Text in runs of their own style: bold here, a colour there, a link.
//!
//! A `Text` node takes it two ways. `spans` is a list whose entries are
//! strings or `{ text, bold, italic, underline, strike, color, size, family,
//! weight, link }` tables; `markup` is the small subset of HTML the desktop
//! notification spec allows — `<b>`, `<i>`, `<u>`, `<s>`, `<a href>`, `<br>`
//! and entities — which is what a notification's body is written in. Either
//! comes out as one [`RichText`]: the plain text, and the runs over it.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;

use crate::types::{Color, Value};

/// One run of text with its own style. Anything `None` is the node's own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichSpan {
    /// The bytes of [`RichText::text`] it covers.
    pub range: Range<usize>,
    pub weight: Option<u16>,
    pub italic: Option<bool>,
    pub underline: bool,
    pub strike: bool,
    pub color: Option<Color>,
    pub size: Option<f64>,
    pub family: Option<String>,
    /// Where it leads when clicked; a link is underlined unless it says not.
    pub link: Option<String>,
}

/// Plain text and the styled runs over it, in order, covering all of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichText {
    pub text: String,
    pub spans: Vec<RichSpan>,
    /// A hash of all of the above, for caches that key on it.
    pub key: u64,
}

/// The most runs one node may hold.
pub const MAX_SPANS: usize = 4096;

impl RichText {
    fn seal(mut self) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.text.hash(&mut hasher);
        for span in &self.spans {
            span.range.hash(&mut hasher);
            span.weight.hash(&mut hasher);
            span.italic.hash(&mut hasher);
            span.underline.hash(&mut hasher);
            span.strike.hash(&mut hasher);
            span.color
                .map(|color| [color.red, color.green, color.blue, color.alpha].map(f32::to_bits))
                .hash(&mut hasher);
            span.size.map(f64::to_bits).hash(&mut hasher);
            span.family.hash(&mut hasher);
            span.link.hash(&mut hasher);
        }
        self.key = hasher.finish().max(1);
        self
    }

    /// Whether any run is a link.
    pub fn has_links(&self) -> bool {
        self.spans.iter().any(|span| span.link.is_some())
    }

    /// Reads a node's `spans` value: `None` when there are none.
    pub fn from_spans(value: &Value) -> Result<Option<Self>, String> {
        let items = match value {
            Value::Nil => return Ok(None),
            Value::List(items) if items.is_empty() => return Ok(None),
            Value::Map(entries) if entries.is_empty() => return Ok(None),
            Value::List(items) => items,
            _ => return Err("spans is a list of strings and { text = ... } tables".to_owned()),
        };
        if items.len() > MAX_SPANS {
            return Err(format!("at most {MAX_SPANS} spans"));
        }
        let mut rich = RichText::default();
        for item in items {
            let (text, mut span) = match item {
                Value::String(text) => (text.as_str(), RichSpan::default()),
                Value::Map(fields) => span_fields(fields)?,
                _ => return Err("a span is a string or a { text = ... } table".to_owned()),
            };
            let start = rich.text.len();
            rich.text.push_str(text);
            span.range = start..rich.text.len();
            if !span.range.is_empty() {
                rich.spans.push(span);
            }
        }
        Ok(Some(rich.seal()))
    }

    /// A text input's `highlights` over `text`: `{ start, stop, color,
    /// underline, strike }` tables, byte offsets, in any order. Only what
    /// leaves every glyph where it was may be set -- a colour, a line under
    /// or through -- so the caret, the selection and a click land where the
    /// field's own layout put them. A range that overlaps an earlier one,
    /// or splits a character, is cut back to fit. `None` when none apply.
    pub fn from_highlights(text: &str, value: &Value) -> Result<Option<Self>, String> {
        let items = match value {
            Value::Nil => return Ok(None),
            Value::List(items) if items.is_empty() => return Ok(None),
            Value::Map(entries) if entries.is_empty() => return Ok(None),
            Value::List(items) => items,
            _ => return Err("highlights is a list of { start, stop, color } tables".to_owned()),
        };
        if items.len() > MAX_SPANS {
            return Err(format!("at most {MAX_SPANS} highlights"));
        }
        let mut marks = Vec::with_capacity(items.len());
        for item in items {
            let Value::Map(fields) = item else {
                return Err("a highlight is a { start, stop, color } table".to_owned());
            };
            let number = |name: &str| match fields.get(name) {
                Some(Value::Number(n)) if n.is_finite() => Some(n.max(0.0) as usize),
                _ => None,
            };
            let (Some(start), Some(stop)) = (number("start"), number("stop")) else {
                return Err("a highlight needs start and stop byte offsets".to_owned());
            };
            let boundary = |mut at: usize| {
                at = at.min(text.len());
                while !text.is_char_boundary(at) {
                    at -= 1;
                }
                at
            };
            let (start, stop) = (boundary(start), boundary(stop));
            if start >= stop {
                continue;
            }
            let color = match fields.get("color") {
                Some(Value::Color(color)) => Some(*color),
                Some(Value::String(name)) => {
                    Some(Color::parse(name).ok_or_else(|| format!("highlight colour `{name}` is not a colour"))?)
                }
                _ => None,
            };
            let flag = |name: &str| matches!(fields.get(name), Some(Value::Bool(true)));
            marks.push(RichSpan {
                range: start..stop,
                color,
                underline: flag("underline"),
                strike: flag("strike"),
                ..RichSpan::default()
            });
        }
        if marks.is_empty() {
            return Ok(None);
        }
        marks.sort_by_key(|mark| mark.range.start);
        // The runs cover all of it: the gaps between marks are plain.
        let mut spans = Vec::with_capacity(marks.len() * 2 + 1);
        let mut at = 0;
        for mut mark in marks {
            mark.range.start = mark.range.start.max(at);
            if mark.range.start >= mark.range.end {
                continue;
            }
            if mark.range.start > at {
                spans.push(RichSpan { range: at..mark.range.start, ..RichSpan::default() });
            }
            at = mark.range.end;
            spans.push(mark);
        }
        if at < text.len() {
            spans.push(RichSpan { range: at..text.len(), ..RichSpan::default() });
        }
        Ok(Some(RichText { text: text.to_owned(), spans, key: 0 }.seal()))
    }

    /// Reads notification-style markup. Never fails: what is not markup is
    /// text, and an unknown tag is dropped with its content kept.
    pub fn from_markup(markup: &str) -> Self {
        let mut rich = RichText::default();
        // The style open at this point, innermost last.
        let mut open: Vec<(&'static str, Option<String>)> = Vec::new();
        let mut rest = markup;
        let mut pending = String::new();
        let flush =
            |rich: &mut RichText, pending: &mut String, open: &[(&'static str, Option<String>)]| {
                if pending.is_empty() {
                    return;
                }
                let start = rich.text.len();
                rich.text.push_str(pending);
                pending.clear();
                let mut span = RichSpan {
                    range: start..rich.text.len(),
                    ..RichSpan::default()
                };
                for (tag, href) in open {
                    match *tag {
                        "b" => span.weight = Some(700),
                        "i" => span.italic = Some(true),
                        "u" => span.underline = true,
                        "s" => span.strike = true,
                        "a" => {
                            span.link = href.clone();
                            span.underline = true;
                        }
                        _ => {}
                    }
                }
                // Runs of one style join, so plain text around a tag is one run.
                if let Some(last) = rich.spans.last_mut()
                    && last.range.end == span.range.start
                    && (RichSpan {
                        range: span.range.clone(),
                        ..last.clone()
                    }) == span
                {
                    last.range.end = span.range.end;
                } else {
                    rich.spans.push(span);
                }
            };
        while !rest.is_empty() {
            if rich.spans.len() >= MAX_SPANS {
                pending.push_str(&decode_entities(rest));
                break;
            }
            let Some(at) = rest.find(['<', '&']) else {
                pending.push_str(rest);
                break;
            };
            pending.push_str(&rest[..at]);
            rest = &rest[at..];
            if rest.starts_with('&') {
                let (text, used) = entity(rest);
                pending.push_str(&text);
                rest = &rest[used..];
                continue;
            }
            let Some(end) = rest.find('>') else {
                // A `<` that opens nothing is a less-than sign.
                pending.push_str(rest);
                break;
            };
            let tag = &rest[1..end];
            rest = &rest[end + 1..];
            let closing = tag.starts_with('/');
            let body = tag.trim_start_matches('/').trim_end_matches('/').trim();
            let name = body
                .split(|c: char| c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if name == "br" {
                pending.push('\n');
                continue;
            }
            let known: Option<&'static str> = match name.as_str() {
                "b" | "strong" => Some("b"),
                "i" | "em" => Some("i"),
                "u" => Some("u"),
                "s" | "strike" | "del" => Some("s"),
                "a" => Some("a"),
                _ => None,
            };
            let Some(known) = known else {
                // `<img>` and the rest: nothing to draw, the content stays.
                continue;
            };
            flush(&mut rich, &mut pending, &open);
            if closing {
                if let Some(index) = open.iter().rposition(|(tag, _)| *tag == known) {
                    open.remove(index);
                }
            } else if !tag.ends_with('/') {
                let href = (known == "a").then(|| attribute(body, "href")).flatten();
                open.push((known, href));
            }
        }
        flush(&mut rich, &mut pending, &open);
        rich.seal()
    }
}

fn span_fields(fields: &BTreeMap<String, Value>) -> Result<(&str, RichSpan), String> {
    for key in fields.keys() {
        if !matches!(
            key.as_str(),
            "text"
                | "bold"
                | "italic"
                | "underline"
                | "strike"
                | "color"
                | "size"
                | "family"
                | "weight"
                | "link"
        ) {
            return Err(format!("a span has no `{key}`"));
        }
    }
    let text = match fields.get("text") {
        Some(Value::String(text)) => text.as_str(),
        None => "",
        Some(_) => return Err("a span's text must be a string".to_owned()),
    };
    let flag = |name: &str| -> Result<Option<bool>, String> {
        match fields.get(name) {
            None => Ok(None),
            Some(Value::Bool(value)) => Ok(Some(*value)),
            Some(_) => Err(format!("a span's {name} must be a boolean")),
        }
    };
    let mut span = RichSpan {
        italic: flag("italic")?,
        underline: flag("underline")?.unwrap_or(false),
        strike: flag("strike")?.unwrap_or(false),
        ..RichSpan::default()
    };
    if flag("bold")? == Some(true) {
        span.weight = Some(700);
    }
    match fields.get("weight") {
        None => {}
        Some(Value::Number(weight)) if weight.is_finite() && (1.0..=1000.0).contains(weight) => {
            span.weight = Some(*weight as u16);
        }
        Some(_) => return Err("a span's weight must be 1..1000".to_owned()),
    }
    span.color = match fields.get("color") {
        None => None,
        Some(Value::Color(color)) => Some(*color),
        Some(Value::String(text)) => {
            Some(Color::parse(text).ok_or_else(|| format!("`{text}` is not a colour"))?)
        }
        Some(_) => return Err("a span's color must be a colour".to_owned()),
    };
    span.size = match fields.get("size") {
        None => None,
        Some(Value::Number(size)) if size.is_finite() && *size > 0.0 && *size <= 1024.0 => {
            Some(*size)
        }
        Some(_) => return Err("a span's size must be a positive number".to_owned()),
    };
    span.family = match fields.get("family") {
        None => None,
        Some(Value::String(family)) => Some(family.clone()),
        Some(_) => return Err("a span's family must be a string".to_owned()),
    };
    span.link = match fields.get("link") {
        None => None,
        Some(Value::String(link)) => Some(link.clone()),
        Some(_) => return Err("a span's link must be a string".to_owned()),
    };
    if span.link.is_some() && !fields.contains_key("underline") {
        span.underline = true;
    }
    Ok((text, span))
}

/// `name="value"` or `name='value'` in a tag's body, entities decoded.
fn attribute(body: &str, name: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    let mut search = 0;
    while let Some(found) = lower[search..].find(name) {
        let at = search + found;
        search = at + name.len();
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let after = body[search..].trim_start();
        if !before_ok || !after.starts_with('=') {
            continue;
        }
        let value = after[1..].trim_start();
        let quote = value.chars().next()?;
        if quote == '"' || quote == '\'' {
            let end = value[1..].find(quote)?;
            return Some(decode_entities(&value[1..1 + end]));
        }
        let end = value.find(char::is_whitespace).unwrap_or(value.len());
        return Some(decode_entities(&value[..end]));
    }
    None
}

/// One entity at the start of `text`, and how many bytes it took; a lone
/// `&` is itself.
fn entity(text: &str) -> (String, usize) {
    let Some(end) = text[..text.len().min(12)].find(';') else {
        return ("&".to_owned(), 1);
    };
    let name = &text[1..end];
    let decoded = match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => name
            .strip_prefix("#x")
            .or_else(|| name.strip_prefix("#X"))
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .or_else(|| name.strip_prefix('#').and_then(|dec| dec.parse().ok()))
            .and_then(char::from_u32),
    };
    match decoded {
        Some(character) => (character.to_string(), end + 1),
        None => ("&".to_owned(), 1),
    }
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let (decoded, used) = entity(&rest[at..]);
        out.push_str(&decoded);
        rest = &rest[at + used..];
    }
    out.push_str(rest);
    out
}

pub(crate) fn canonical_spans(value: Value) -> Result<Value, String> {
    RichText::from_spans(&value)?;
    Ok(match value {
        Value::Map(entries) if entries.is_empty() => Value::List(Vec::new()),
        Value::Nil => Value::List(Vec::new()),
        value => value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    type Run<'a> = (&'a str, Option<u16>, Option<bool>, bool, Option<&'a str>);

    fn runs(rich: &RichText) -> Vec<Run<'_>> {
        rich.spans
            .iter()
            .map(|span| {
                (
                    &rich.text[span.range.clone()],
                    span.weight,
                    span.italic,
                    span.underline,
                    span.link.as_deref(),
                )
            })
            .collect()
    }

    #[test]
    fn markup_nests_and_decodes() {
        let rich = RichText::from_markup(
            "Hi <b>bold <i>both</i></b> &amp; <a href=\"https://x.org/?a=1&amp;b=2\">here</a>&#33;<br/>x < y",
        );
        assert_eq!(rich.text, "Hi bold both & here!\nx < y");
        assert_eq!(
            runs(&rich),
            vec![
                ("Hi ", None, None, false, None),
                ("bold ", Some(700), None, false, None),
                ("both", Some(700), Some(true), false, None),
                (" & ", None, None, false, None),
                ("here", None, None, true, Some("https://x.org/?a=1&b=2")),
                ("!\nx < y", None, None, false, None),
            ]
        );
        // Unknown tags go and their content stays; stray closers do nothing.
        let odd = RichText::from_markup("<img src='a.png'/>pic</u><span>ok</span> &bogus; &");
        assert_eq!(odd.text, "picok &bogus; &");
        assert_eq!(RichText::from_markup("plain").spans.len(), 1);
        assert_eq!(RichText::from_markup("").spans.len(), 0);
        assert_ne!(
            RichText::from_markup("<b>a</b>").key,
            RichText::from_markup("a").key
        );
    }

    #[test]
    fn spans_read_strings_and_tables() {
        let mut bold = BTreeMap::new();
        bold.insert("text".to_owned(), Value::String("b".to_owned()));
        bold.insert("bold".to_owned(), Value::Bool(true));
        bold.insert("color".to_owned(), Value::String("#ff0000".to_owned()));
        let mut link = BTreeMap::new();
        link.insert("text".to_owned(), Value::String("go".to_owned()));
        link.insert("link".to_owned(), Value::String("https://a".to_owned()));
        let value = Value::List(vec![
            Value::String("a".to_owned()),
            Value::Map(bold),
            Value::String(String::new()),
            Value::Map(link),
        ]);
        let rich = RichText::from_spans(&value).unwrap().unwrap();
        assert_eq!(rich.text, "abgo");
        assert_eq!(rich.spans.len(), 3);
        assert_eq!(rich.spans[1].weight, Some(700));
        assert_eq!(rich.spans[1].color, Color::parse("#ff0000"));
        assert!(rich.spans[2].underline && rich.has_links());
        assert!(
            RichText::from_spans(&Value::List(vec![]))
                .unwrap()
                .is_none()
        );
        let mut bad = BTreeMap::new();
        bad.insert("colour".to_owned(), Value::String("red".to_owned()));
        assert!(
            RichText::from_spans(&Value::List(vec![Value::Map(bad)]))
                .unwrap_err()
                .contains("colour")
        );
    }
}

#[cfg(test)]
mod highlight_tests {
    use super::*;

    fn mark(start: f64, stop: f64, color: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("start".to_owned(), Value::Number(start)),
            ("stop".to_owned(), Value::Number(stop)),
            ("color".to_owned(), Value::String(color.to_owned())),
        ]))
    }

    #[test]
    fn highlights_cover_the_text_in_order_and_cut_overlaps() {
        let text = "local x = 1";
        let rich = RichText::from_highlights(text, &Value::List(vec![mark(8.0, 9.0, "#ff0000"), mark(0.0, 5.0, "#0000ff"), mark(3.0, 7.0, "#00ff00")]))
            .unwrap()
            .unwrap();
        let ranges: Vec<_> = rich.spans.iter().map(|s| s.range.clone()).collect();
        assert_eq!(ranges, vec![0..5, 5..7, 7..8, 8..9, 9..11]);
        assert!(rich.spans[1].color.is_some() && rich.spans[2].color.is_none());
        assert_eq!(RichText::from_highlights(text, &Value::List(Vec::new())).unwrap(), None);
    }
}
