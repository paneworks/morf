//! `morf.text`: working with strings a person typed or will read.
//!
//! ```lua
//! local hits = morf.text.fuzzy("ffx", apps, { key = { "name", { "exec", 0.5 } }, limit = 20 })
//! for _, hit in ipairs(hits) do
//!   -- hit.item, hit.index, hit.score, hit.key, hit.positions
//! end
//! local score, positions = morf.text.fuzzy_score("ffx", "Firefox")
//! local spans = morf.text.highlight("Firefox", positions, { bold = true })
//! ```
//!
//! Positions are 1-based byte offsets into the matched string, the way
//! `string.sub` counts, and each marks the first byte of a matched
//! character. The matcher itself is `morf_text::fuzzy`.

use std::borrow::Cow;

use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};
use morf_text::fuzzy::{FuzzyMatcher, rank};

use crate::scene_bindings::*;

/// The most items one call ranks.
const MAX_ITEMS: usize = 1_000_000;
/// The most keys an item is matched by.
const MAX_KEYS: usize = 16;

fn text_of<'gc>(value: &LuaValue<'gc>) -> Option<Cow<'gc, str>> {
    match value {
        LuaValue::String(text) => Some(String::from_utf8_lossy(text.as_bytes())),
        _ => None,
    }
}

/// `key = "name"`, `key = { "name", "exec" }`, or entries `{ "exec", 0.5 }`
/// with a weight.
fn keys_of<'gc>(ctx: Context<'gc>, value: LuaValue<'gc>) -> Result<Vec<(String, f64)>, String> {
    let entry = |value: LuaValue<'gc>| -> Result<(String, f64), String> {
        match value {
            LuaValue::String(name) => Ok((name.display_lossy().to_string(), 1.0)),
            LuaValue::Table(pair) => {
                let name = match pair.get_value(ctx, 1) {
                    LuaValue::String(name) => name.display_lossy().to_string(),
                    _ => return Err("a weighted key is { \"field\", weight }".into()),
                };
                let weight = match pair.get_value(ctx, 2) {
                    LuaValue::Nil => 1.0,
                    LuaValue::Integer(weight) => weight as f64,
                    LuaValue::Number(weight) if weight.is_finite() => weight,
                    _ => return Err(format!("key `{name}` has a weight that is not a number")),
                };
                if weight <= 0.0 {
                    return Err(format!("key `{name}` needs a weight above zero"));
                }
                Ok((name, weight))
            }
            _ => Err("a key is a field name or { \"field\", weight }".into()),
        }
    };
    match value {
        LuaValue::Nil => Ok(Vec::new()),
        LuaValue::String(_) => Ok(vec![entry(value)?]),
        LuaValue::Table(list) => {
            let count = usize::try_from(list.length(&ctx)).unwrap_or(0);
            if count == 0 || count > MAX_KEYS {
                return Err(format!("fuzzy key lists hold 1 to {MAX_KEYS} keys"));
            }
            (1..=count as i64)
                .map(|index| entry(list.get_value(ctx, index)))
                .collect()
        }
        _ => Err("fuzzy key must be a field name or a list of them".into()),
    }
}

fn positions_table<'gc>(ctx: Context<'gc>, positions: &[usize]) -> Table<'gc> {
    let table = Table::new(&ctx);
    for (index, position) in positions.iter().enumerate() {
        let _ = table.set(ctx, index as i64 + 1, *position as i64 + 1);
    }
    table
}

pub(crate) fn install_text_api<'gc>(ctx: Context<'gc>, morf: Table<'gc>) {
    let text = Table::new(&ctx);

    let fuzzy = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (query, items, options): (LuaValue, Table, Option<Table>) = stack.consume(ctx)?;
        let query =
            text_of(&query).ok_or_else(|| HostError("fuzzy: the query must be a string".into()))?;
        let (keys, limit) = match options {
            Some(options) => {
                let keys = keys_of(ctx, options.get_value(ctx, "key"))
                    .map_err(|error| HostError(format!("fuzzy: {error}")))?;
                let limit = match options.get_value(ctx, "limit") {
                    LuaValue::Nil => usize::MAX,
                    LuaValue::Integer(limit) if limit >= 0 => limit as usize,
                    LuaValue::Number(limit) if limit.is_finite() && limit >= 0.0 => limit as usize,
                    _ => return Err(HostError("fuzzy: limit must be a count".into()).into()),
                };
                (keys, limit)
            }
            None => (Vec::new(), usize::MAX),
        };
        let count = usize::try_from(items.length(&ctx)).unwrap_or(0);
        if count > MAX_ITEMS {
            return Err(HostError(format!("fuzzy: more than {MAX_ITEMS} items")).into());
        }
        let mut values = Vec::with_capacity(count);
        let mut candidates: Vec<Vec<(Cow<str>, f64)>> = Vec::with_capacity(count);
        for index in 1..=count as i64 {
            let item = items.get_value(ctx, index);
            let texts = match item {
                LuaValue::String(_) => vec![(text_of(&item).unwrap_or_default(), 1.0)],
                LuaValue::Table(fields) => {
                    if keys.is_empty() {
                        return Err(HostError(
                            "fuzzy: the items are tables; say which field to match with `key`"
                                .into(),
                        )
                        .into());
                    }
                    keys.iter()
                        .map(|(name, weight)| {
                            // A missing field matches nothing.
                            let value = fields.get_value(ctx, name.as_str());
                            (text_of(&value).unwrap_or_default(), *weight)
                        })
                        .collect()
                }
                LuaValue::Nil => break,
                other => {
                    return Err(HostError(format!(
                        "fuzzy: item {index} is a {}, not a string or a table",
                        other.type_name()
                    ))
                    .into());
                }
            };
            values.push(item);
            candidates.push(texts);
        }
        let ranked = rank(&query, &candidates, limit);
        let result = Table::new(&ctx);
        for (slot, hit) in ranked.iter().enumerate() {
            let entry = Table::new(&ctx);
            entry.set_field(ctx, "item", values[hit.index]);
            entry.set_field(ctx, "index", hit.index as i64 + 1);
            entry.set_field(ctx, "score", hit.score);
            if let Some((name, _)) = keys.get(hit.key) {
                entry.set_field(ctx, "key", name.as_str());
            }
            entry.set_field(ctx, "positions", positions_table(ctx, &hit.positions));
            result
                .set(ctx, slot as i64 + 1, entry)
                .map_err(|error| HostError(error.to_string()))?;
        }
        stack.replace(ctx, result);
        Ok(CallbackReturn::Return)
    });
    text.set_field(ctx, "fuzzy", fuzzy);

    let fuzzy_score = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (query, subject): (LuaValue, LuaValue) = stack.consume(ctx)?;
        let (Some(query), Some(subject)) = (text_of(&query), text_of(&subject)) else {
            return Err(HostError("fuzzy_score takes two strings".into()).into());
        };
        match FuzzyMatcher::new(&query).find(&subject) {
            Some(found) => stack.replace(
                ctx,
                (
                    i64::from(found.score),
                    positions_table(ctx, &found.positions),
                ),
            ),
            None => stack.replace(ctx, LuaValue::Nil),
        }
        Ok(CallbackReturn::Return)
    });
    text.set_field(ctx, "fuzzy_score", fuzzy_score);

    // `highlight(text, positions, style)`: spans for a `ui.Text`, the
    // characters at `positions` in runs styled by `style`, the rest plain.
    let highlight = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (subject, positions, style): (luna::String, Option<Table>, Option<Table>) =
            stack.consume(ctx)?;
        let bytes = subject.as_bytes();
        let subject = String::from_utf8_lossy(bytes);
        let mut marked = vec![false; subject.len()];
        if let Some(positions) = positions {
            for index in 1..=positions.length(&ctx) {
                if let LuaValue::Integer(position) = positions.get_value(ctx, index)
                    && position >= 1
                    && (position as usize) <= subject.len()
                {
                    marked[position as usize - 1] = true;
                }
            }
        }
        let spans = Table::new(&ctx);
        let mut slot = 1;
        let mut push = |piece: &str, hit: bool| -> Result<(), HostError> {
            if piece.is_empty() {
                return Ok(());
            }
            let value: LuaValue = match (hit, style) {
                (true, Some(style)) => {
                    let span = Table::new(&ctx);
                    for (key, value) in style.iter(ctx) {
                        span.set(ctx, key, value)
                            .map_err(|error| HostError(error.to_string()))?;
                    }
                    span.set_field(ctx, "text", ctx.intern(piece.as_bytes()));
                    span.into()
                }
                (true, None) => {
                    let span = Table::new(&ctx);
                    span.set_field(ctx, "text", ctx.intern(piece.as_bytes()));
                    span.set_field(ctx, "bold", true);
                    span.into()
                }
                (false, _) => ctx.intern(piece.as_bytes()).into(),
            };
            spans
                .set(ctx, slot, value)
                .map_err(|error| HostError(error.to_string()))?;
            slot += 1;
            Ok(())
        };
        // Runs of matched characters, and of the rest.
        let mut run_start = 0;
        let mut run_hit = false;
        for (offset, _) in subject.char_indices() {
            let hit = marked[offset];
            if offset > 0 && hit != run_hit {
                push(&subject[run_start..offset], run_hit)?;
                run_start = offset;
            }
            run_hit = hit;
        }
        push(&subject[run_start..], run_hit)?;
        stack.replace(ctx, spans);
        Ok(CallbackReturn::Return)
    });
    text.set_field(ctx, "highlight", highlight);

    morf.set_field(ctx, "text", text);
}
