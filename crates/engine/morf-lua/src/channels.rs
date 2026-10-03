//! `morf.channel`: data channels (`morf_scene::channel`) as Lua sees them,
//! and what a turn of the loop does when one was written.
//!
//! A channel handle reads reactively: a binding that called `ch:get()`,
//! `ch:last()` or `ch:peak()` runs again when the channel is written, by
//! Lua or by a Rust producer. A `ui.Path` with `series = ch` draws it with
//! no Lua at all; the loop repaints only when a channel such a path shows
//! has moved.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};
use morf_scene::reactive::SignalId;
use morf_scene::{Channel, NodeHandle, Value as SceneValue};

use crate::scene_bindings::HostError;
use crate::state::ReactiveState;
use crate::surface_types::IpcValue;
use crate::types::Runtime;

/// One runtime's view of the channels.
#[derive(Default)]
pub(crate) struct Channels {
    /// The channels handles were made for, the signal reads depend on, and
    /// the revision that signal last took.
    watched: HashMap<u64, (Arc<Channel>, SignalId, u64)>,
    /// Nodes given a `series`, and the revision each last drew.
    series_nodes: HashMap<NodeHandle, u64>,
    /// The channels' generation this runtime last looked at.
    seen: u64,
}

impl Channels {
    /// A node was given a `series`: watched for repaints from now on.
    pub(crate) fn note_series(&mut self, node: NodeHandle) {
        self.series_nodes.entry(node).or_insert(u64::MAX);
    }
}

fn channel_id(value: &SceneValue) -> Option<u64> {
    match value {
        SceneValue::Number(n) if *n >= 1.0 => Some(*n as u64),
        SceneValue::Map(fields) => match fields.get("id") {
            Some(SceneValue::Number(n)) if *n >= 1.0 => Some(*n as u64),
            _ => None,
        },
        _ => None,
    }
}

impl Runtime {
    /// Takes what the channels' writers did since the last turn: wakes the
    /// bindings that read a written channel, and says whether a path that
    /// draws one is on show (the loop repaints for it).
    pub(crate) fn poll_channels(&mut self) -> bool {
        let generation = morf_scene::channels_generation();
        let flush = {
            let mut guard = self.reactive.borrow_mut();
            let state = &mut *guard;
            if generation == state.channels.seen {
                return false;
            }
            state.channels.seen = generation;
            let mut flush = false;
            let mut writes = Vec::new();
            for (channel, signal, last) in state.channels.watched.values_mut() {
                let revision = channel.revision();
                if revision != *last {
                    *last = revision;
                    writes.push((*signal, IpcValue::Integer(revision as i64)));
                }
            }
            if let Some(graph) = state.graph.as_mut() {
                for (signal, value) in writes {
                    if graph.write(signal, value.clone()).is_ok() {
                        state.values.insert(signal, value);
                        flush = true;
                    }
                }
            }
            // A path on show whose channel moved: a frame.
            let mut shown = false;
            let scene = &state.scene;
            state.channels.series_nodes.retain(|node, drawn| {
                if !scene.contains(*node) {
                    return false;
                }
                let Some(channel) = scene
                    .current(*node, "series")
                    .ok()
                    .and_then(channel_id)
                    .and_then(morf_scene::channel_by_id)
                else {
                    return true;
                };
                let revision = channel.revision();
                if revision != *drawn {
                    *drawn = revision;
                    shown |= scene.change_is_shown(*node, "series");
                }
                true
            });
            if shown {
                state.scene_revision = state.scene_revision.wrapping_add(1);
            }
            (flush, shown)
        };
        let (flush, shown) = flush;
        if flush {
            let limits = self.limits;
            let reactive = Rc::clone(&self.reactive);
            self.lua.enter(|ctx| {
                if let Err(message) = crate::reactive_bindings::flush_reactive(&reactive, ctx, limits) {
                    reactive.borrow_mut().log(crate::LogLevel::Warn, format!("channel: {message}"));
                }
            });
        }
        shown || flush
    }
}

/// The signal reads of `channel` depend on, made on first use.
fn watch(state: &mut ReactiveState, channel: &Arc<Channel>) -> Result<SignalId, HostError> {
    if let Some((_, signal, _)) = state.channels.watched.get(&channel.id()) {
        return Ok(*signal);
    }
    let revision = channel.revision();
    let value = IpcValue::Integer(revision as i64);
    let signal = state
        .graph
        .as_mut()
        .ok_or_else(|| HostError("reactive graph is already running".to_owned()))?
        .signal(format!("channel.{}", channel.id()), value.clone());
    state.values.insert(signal, value);
    state.signals.push(signal);
    state.channels.watched.insert(channel.id(), (Arc::clone(channel), signal, revision));
    Ok(signal)
}

fn track(state: &Rc<RefCell<ReactiveState>>, signal: SignalId) {
    if let Some(active) = &mut state.borrow_mut().active {
        active.reads.insert(signal);
    }
}

fn number(value: LuaValue<'_>) -> f32 {
    match value {
        LuaValue::Integer(n) => n as f32,
        LuaValue::Number(n) => n as f32,
        _ => 0.0,
    }
}

/// Installs `morf.channel { name, size = 60, mode = "ring" | "frame" }`.
pub(crate) fn install<'gc>(ctx: Context<'gc>, state: Rc<RefCell<ReactiveState>>, morf: Table<'gc>) {
    let make = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let options: Option<Table> = stack.consume(ctx)?;
        let field = |key: &str| options.map_or(LuaValue::Nil, |o| o.get_value(ctx, key));
        let name = match field("name") {
            LuaValue::String(s) => Some(s.display_lossy().to_string()),
            _ => None,
        };
        let size = match field("size") {
            LuaValue::Integer(n) => n.max(1) as usize,
            LuaValue::Number(n) => n.max(1.0) as usize,
            LuaValue::Nil => 60,
            _ => return Err(HostError("channel size must be a number".into()).into()),
        };
        let ring = match field("mode") {
            LuaValue::String(s) => match s.to_str().unwrap_or("") {
                "ring" => true,
                "frame" => false,
                _ => return Err(HostError("channel mode is ring or frame".into()).into()),
            },
            _ => true,
        };
        let channel = morf_scene::channel(name.as_deref(), size, ring);
        let signal = watch(&mut state.borrow_mut(), &channel)?;
        let handle = Table::new(&ctx);
        handle.set_field(ctx, "id", channel.id() as i64);
        handle.set_field(ctx, "size", channel.capacity() as i64);
        {
            let c = Arc::clone(&channel);
            handle.set_field(ctx, "push", Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let (_, value): (LuaValue, LuaValue) = stack.consume(ctx)?;
                // A list pushes each of its numbers: a spectrogram's column.
                if let LuaValue::Table(list) = value {
                    for i in 1..=morf_scene::MAX_CHANNEL_LEN as i64 {
                        match list.get_value(ctx, i) {
                            LuaValue::Nil => break,
                            v => c.push(number(v)),
                        }
                    }
                } else {
                    c.push(number(value));
                }
                Ok(CallbackReturn::Return)
            }));
        }
        {
            let c = Arc::clone(&channel);
            handle.set_field(ctx, "set", Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let (_, list): (LuaValue, Option<Table>) = stack.consume(ctx)?;
                let mut values = Vec::new();
                if let Some(list) = list {
                    for i in 1..=morf_scene::MAX_CHANNEL_LEN as i64 {
                        match list.get_value(ctx, i) {
                            LuaValue::Nil => break,
                            v => values.push(number(v)),
                        }
                    }
                }
                c.set(&values);
                Ok(CallbackReturn::Return)
            }));
        }
        {
            let c = Arc::clone(&channel);
            handle.set_field(ctx, "clear", Callback::from_fn(&ctx, move |_, _, mut stack| {
                stack.clear();
                c.clear();
                Ok(CallbackReturn::Return)
            }));
        }
        {
            let (c, s) = (Arc::clone(&channel), Rc::clone(&state));
            handle.set_field(ctx, "get", Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                track(&s, signal);
                let list = Table::new(&ctx);
                for (i, v) in c.snapshot().0.into_iter().enumerate() {
                    list.set(ctx, i as i64 + 1, v as f64)?;
                }
                stack.replace(ctx, list);
                Ok(CallbackReturn::Return)
            }));
        }
        {
            let (c, s) = (Arc::clone(&channel), Rc::clone(&state));
            handle.set_field(ctx, "last", Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                track(&s, signal);
                match c.last() {
                    Some(v) => stack.replace(ctx, v as f64),
                    None => stack.replace(ctx, LuaValue::Nil),
                }
                Ok(CallbackReturn::Return)
            }));
        }
        {
            let (c, s) = (Arc::clone(&channel), Rc::clone(&state));
            handle.set_field(ctx, "peak", Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                track(&s, signal);
                match c.peak() {
                    Some(v) => stack.replace(ctx, v as f64),
                    None => stack.replace(ctx, LuaValue::Nil),
                }
                Ok(CallbackReturn::Return)
            }));
        }
        {
            let (c, s) = (Arc::clone(&channel), Rc::clone(&state));
            handle.set_field(ctx, "len", Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                track(&s, signal);
                stack.replace(ctx, c.len() as i64);
                Ok(CallbackReturn::Return)
            }));
        }
        stack.replace(ctx, handle);
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "channel", make);
}
