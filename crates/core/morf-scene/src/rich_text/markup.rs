//! Notification-style markup: the tags and entities the desktop notification
//! spec allows, read into a [`RichText`].

use super::{MAX_SPANS, RichSpan, RichText};

impl RichText {
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
