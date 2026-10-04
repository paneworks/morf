//! `loop`: motion a node keeps up on its own, for as long as it is asked to.
//!
//! A pet's bob, a spinner's turn, a pulse on a waiting dot. Each is one
//! property travelling between two values over and over, and before this each
//! was a `Timer` restarting a `morf.animation.play` — a Lua call per cycle,
//! and a group to stop by hand when the node went. A loop is declared on the
//! node instead, runs in Rust between frames, and ends with the node.
//!
//! ```lua
//! ui.Item {
//!   loop = { translate_y = { from = 0, to = -4, duration = 1400,
//!                            easing = "in_out_sine", alternate = true } },
//! }
//! ```
//!
//! `loop` may be a binding. Returning a different table restarts what changed,
//! returning nil or `{}` ends every loop, and a loop that ends puts its
//! property back where the loop started it (through the property's own
//! `behavior`, when it has one) -- unless it says `hold = true`, when the
//! property stays wherever the motion had it.

use std::collections::BTreeMap;
use std::time::Duration;

use morf_scene::{Behavior, NodeHandle, Repeat, Value};

use super::AnimationHost;
use super::easing::easing_from_scene;

/// One property a node is looping, and where it goes back to.
#[derive(Clone, Debug)]
pub struct RunningLoop {
    /// The declaration it was started from, so a binding that returns the
    /// same table again leaves the motion alone instead of restarting it.
    spec: Value,
    /// Where the property rests once the loop ends: its `from`.
    rest: Value,
    /// `hold = true`: an ended loop leaves the property where the motion
    /// had it, instead of taking it back to `rest`.
    hold: bool,
}

/// Starts, keeps, restarts or ends a node's loops to match `value`.
pub fn apply_loops(
    host: &mut dyn AnimationHost,
    node: NodeHandle,
    value: &Value,
) -> Result<(), String> {
    let wanted = match value {
        Value::Nil => BTreeMap::new(),
        Value::Map(entries) => entries.clone(),
        Value::List(entries) if entries.is_empty() => BTreeMap::new(),
        _ => return Err("loop must be a property-keyed table".to_owned()),
    };
    let mut previous = host.animation().loops.remove(&node).unwrap_or_default();
    for (property, ended) in &previous {
        if wanted.contains_key(property) {
            continue;
        }
        // Stopping leaves the value where the motion had it, and makes that
        // the target; a held loop ends there.
        host.scene()
            .stop_animation(node, property)
            .map_err(|error| error.to_string())?;
        if !ended.hold {
            host.assign(node, property, ended.rest.clone())?;
        }
    }
    let mut running = BTreeMap::new();
    for (property, spec) in wanted {
        let old = previous.remove(&property);
        if let Some(old) = &old
            && old.spec == spec
            && host
                .scene()
                .is_animating(node, &property)
                .map_err(|error| error.to_string())?
        {
            running.insert(property, old.clone());
            continue;
        }
        let parsed = LoopSpec::parse(&property, &spec)?;
        let hold = parsed.hold;
        let from = match parsed.from {
            Some(from) => from,
            None => match &old {
                Some(old) if !old.hold => old.rest.clone(),
                _ => host
                    .scene()
                    .current(node, &property)
                    .map_err(|error| error.to_string())?
                    .clone(),
            },
        };
        host.scene()
            .animate_from(node, &property, from.clone(), parsed.to, parsed.behavior)
            .map_err(|error| format!("loop `{property}`: {error}"))?;
        running.insert(
            property,
            RunningLoop {
                spec,
                rest: from,
                hold,
            },
        );
    }
    if !running.is_empty() {
        host.animation().loops.insert(node, running);
    }
    Ok(())
}

struct LoopSpec {
    hold: bool,
    from: Option<Value>,
    to: Value,
    behavior: Behavior,
}

impl LoopSpec {
    fn parse(property: &str, spec: &Value) -> Result<Self, String> {
        let Value::Map(fields) = spec else {
            return Err(format!("loop `{property}` must be a table"));
        };
        for key in fields.keys() {
            if !matches!(
                key.as_str(),
                "from" | "to" | "duration" | "easing" | "alternate" | "loops" | "delay" | "hold"
            ) {
                return Err(format!("loop `{property}` has no field `{key}`"));
            }
        }
        let to = fields
            .get("to")
            .cloned()
            .ok_or_else(|| format!("loop `{property}` needs a `to`"))?;
        let milliseconds = |field: &str, required: bool| -> Result<Duration, String> {
            match fields.get(field) {
                Some(Value::Number(value)) if value.is_finite() && *value >= 0.0 => {
                    Ok(Duration::from_secs_f64(value / 1_000.0))
                }
                None if !required => Ok(Duration::ZERO),
                _ => Err(format!(
                    "loop `{property}` {field} must be a non-negative number of milliseconds"
                )),
            }
        };
        if !fields.contains_key("duration") {
            return Err(format!("loop `{property}` needs a duration"));
        }
        let duration = milliseconds("duration", true)?;
        if duration.is_zero() {
            return Err(format!("loop `{property}` needs a duration"));
        }
        let alternate = match fields.get("alternate") {
            None => false,
            Some(Value::Bool(value)) => *value,
            Some(_) => return Err(format!("loop `{property}` alternate must be a boolean")),
        };
        let repeat = match (fields.get("loops"), alternate) {
            (None, false) => Repeat::Forever,
            (None, true) => Repeat::PingPong,
            (Some(Value::String(name)), false) if name == "forever" => Repeat::Forever,
            (Some(Value::String(name)), true) if name == "forever" => Repeat::PingPong,
            (Some(Value::Number(count)), alternate) if count.is_finite() && *count >= 1.0 => {
                match alternate {
                    true => Repeat::PingPongTimes(*count as u32),
                    false => Repeat::Times(*count as u32),
                }
            }
            _ => {
                return Err(format!(
                    "loop `{property}` loops must be a pass count or \"forever\""
                ));
            }
        };
        let hold = match fields.get("hold") {
            None => false,
            Some(Value::Bool(value)) => *value,
            Some(_) => return Err(format!("loop `{property}` hold must be a boolean")),
        };
        Ok(Self {
            hold,
            from: fields.get("from").cloned(),
            to,
            behavior: Behavior {
                duration,
                easing: easing_from_scene(fields.get("easing").unwrap_or(&Value::Nil))
                    .map_err(|error| format!("loop `{property}`: {error}"))?,
                delay: milliseconds("delay", false)?,
                repeat,
                ..Behavior::default()
            },
        })
    }
}
