//! The runtime's side of `ui.Image`: what became of each source, and the
//! playback of the ones that move.
//!
//! Decoding happens where the pixels are drawn, in each surface's renderer,
//! so what the runtime learns it learns after a paint: [`sync`] runs once a
//! frame with the layout and the image cache that frame used. A node reads
//! `loading` until then, `ready` once its source has a size and nothing
//! failed to draw it, and `error` — with the reason in `error` — when the
//! source cannot be read, is not a picture, or would not decode.
//!
//! A moving picture (GIF, animated PNG or WebP) is played by [`advance`] on
//! the main loop, by the wall clock: `frame` moves on when its delay has
//! passed, scaled by `speed`, while `playing` is true and the node was drawn
//! lately. One off screen, hidden, or on a surface that stopped painting
//! stops where it is and costs nothing; a timer wakes the loop only while
//! something is actually playing, and only as often as its frames change.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use luna::StashedClosure;
use morf_image::ImageCache;
use morf_layout::Layout;
use morf_scene::{NodeHandle, Scene, Value as SceneValue};

use crate::scene_bindings::assign_scene_property;
use crate::state::ReactiveState;
use crate::surface_types::IpcValue;

/// A picture not drawn for this long stops playing, whatever the flag the
/// last paint left: its surface is no longer painting.
const SEEN_FOR: Duration = Duration::from_secs(2);
/// The fastest the loop is woken for a moving picture.
const FASTEST_WAKE: Duration = Duration::from_millis(16);

#[derive(Default)]
pub(crate) struct ImageNodes {
    entries: HashMap<NodeHandle, ImageEntry>,
    /// When the quickest playing picture next changes frame, while anything
    /// plays: the loop wakes then, and not before.
    due: Option<Instant>,
    last_advance: Option<Instant>,
}

#[derive(Default)]
struct ImageEntry {
    on_status: Option<StashedClosure>,
    /// The source the status below is about.
    source: Option<String>,
    status: &'static str,
    playback: Option<Playback>,
    /// Drawn in the last paint of its surface, and when that was.
    visible: bool,
    seen: Option<Instant>,
}

struct Playback {
    delays: Vec<Duration>,
    index: usize,
    elapsed: Duration,
    passes: u32,
    finished: bool,
}

/// A callback owed to a configuration: `on_status(status, error)`.
pub(crate) struct StatusCall {
    pub(crate) callback: StashedClosure,
    pub(crate) args: Vec<IpcValue>,
}

impl ImageNodes {
    pub(crate) fn register(&mut self, node: NodeHandle, on_status: Option<StashedClosure>) {
        self.entries.insert(
            node,
            ImageEntry {
                on_status,
                status: "none",
                ..ImageEntry::default()
            },
        );
    }

    pub(crate) fn remove(&mut self, node: NodeHandle) {
        self.entries.remove(&node);
        if self.entries.is_empty() {
            self.due = None;
        }
    }

    /// When a playing picture next needs the loop.
    pub(crate) fn due(&self) -> Option<Instant> {
        self.due
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn set(state: &mut ReactiveState, node: NodeHandle, property: &str, value: SceneValue) {
    let _ = assign_scene_property(state, node, property, value);
}

/// Whether a node and every ancestor are shown and not fully transparent.
fn shown(scene: &Scene, mut node: NodeHandle) -> bool {
    loop {
        if !scene.bool_value(node, "visible").unwrap_or(false)
            || scene.number(node, "opacity").unwrap_or(1.0) <= 0.0
        {
            return false;
        }
        match scene.parent(node) {
            Ok(Some(parent)) => node = parent,
            _ => return true,
        }
    }
}

/// Reads what the last paint made of each image laid out in `layout`;
/// returns the `on_status` calls owed.
pub(crate) fn sync(
    state: &mut ReactiveState,
    layout: &Layout,
    cache: &mut ImageCache,
) -> Vec<StatusCall> {
    let mut calls = Vec::new();
    let now = Instant::now();
    let nodes: Vec<NodeHandle> = state.images.entries.keys().copied().collect();
    for node in nodes {
        let Some(geometry) = layout.geometry(node) else {
            continue;
        };
        let Ok(source) = state.scene.string_value(node, "source").map(str::to_owned) else {
            continue;
        };
        let visible = geometry.width > 0.0 && geometry.height > 0.0 && shown(&state.scene, node);
        let entry = state.images.entries.get_mut(&node).expect("listed above");
        entry.visible = visible;
        entry.seen = Some(now);
        let changed_source = entry.source.as_deref() != Some(source.as_str());
        if changed_source {
            entry.source = Some(source.clone());
            entry.playback = None;
            entry.status = if source.is_empty() { "none" } else { "loading" };
        }
        let before = entry.status;
        let mut error = None;
        if !source.is_empty() && entry.status != "error" {
            match cache.intrinsic_size(&source) {
                Err(failure) => error = Some(failure.to_string()),
                Ok(_) => error = cache.failure(&source).map(str::to_owned),
            }
            if error.is_none() && entry.playback.is_none() {
                match cache.animation(&source) {
                    Some(animation) => {
                        entry.playback = Some(Playback {
                            delays: animation.delays.clone(),
                            index: 0,
                            elapsed: Duration::ZERO,
                            passes: 0,
                            finished: false,
                        });
                    }
                    // Reading the frames can fail where the header did not.
                    None => error = cache.failure(&source).map(str::to_owned),
                }
            }
            entry.status = if error.is_some() { "error" } else { "ready" };
        }
        let status = entry.status;
        let frames = entry
            .playback
            .as_ref()
            .map_or(0, |playback| playback.delays.len());
        // A moving picture just drawn is looked at on the next turn, which
        // works out when its frame changes; until then nothing would wake
        // the loop for it.
        if visible && frames > 1 && state.images.due.is_none() {
            state.images.due = Some(now);
        }
        let callback = entry.on_status.clone();
        if changed_source {
            set(state, node, "frame", SceneValue::Number(0.0));
        }
        if changed_source || status != before {
            set(state, node, "status", SceneValue::String(status.to_owned()));
            set(
                state,
                node,
                "error",
                error.clone().map_or(SceneValue::Nil, SceneValue::String),
            );
            set(
                state,
                node,
                "frame_count",
                SceneValue::Number(frames as f64),
            );
            if let Some(callback) = callback
                && status != before
            {
                calls.push(StatusCall {
                    callback,
                    args: vec![
                        IpcValue::String(status.to_owned()),
                        error.map_or(IpcValue::Nil, IpcValue::String),
                    ],
                });
            }
        }
    }
    calls
}

/// How many passes a node's `loops` asks for; `None` is forever.
fn passes(scene: &Scene, node: NodeHandle) -> Option<u32> {
    match scene.current(node, "loops") {
        Ok(SceneValue::Number(count)) if count.is_finite() && *count >= 1.0 => Some(*count as u32),
        _ => None,
    }
}

/// Moves every playing, drawn picture on by the time since the last call;
/// returns whether any frame changed.
pub(crate) fn advance(state: &mut ReactiveState, now: Instant) -> bool {
    if state.images.entries.is_empty() {
        state.images.due = None;
        state.images.last_advance = None;
        return false;
    }
    let delta = state
        .images
        .last_advance
        .map_or(Duration::ZERO, |last| now.saturating_duration_since(last))
        .min(Duration::from_secs(1));
    state.images.last_advance = Some(now);
    let mut changed = false;
    let mut quickest: Option<Duration> = None;
    let nodes: Vec<NodeHandle> = state.images.entries.keys().copied().collect();
    for node in nodes {
        let scene = &state.scene;
        let Ok(playing) = scene.bool_value(node, "playing") else {
            continue;
        };
        let speed = scene.number(node, "speed").unwrap_or(1.0);
        let frame = scene.number(node, "frame").unwrap_or(0.0);
        let loops = passes(scene, node);
        let entry = state.images.entries.get_mut(&node).expect("listed above");
        let lately = entry
            .seen
            .is_some_and(|seen| now.saturating_duration_since(seen) < SEEN_FOR);
        let Some(playback) = entry.playback.as_mut() else {
            continue;
        };
        let count = playback.delays.len();
        // A write to `frame` is a seek, and a seek after the end plays again.
        let asked = if frame.is_finite() && frame >= 0.0 {
            frame as usize % count
        } else {
            0
        };
        if asked != playback.index {
            playback.index = asked;
            playback.elapsed = Duration::ZERO;
            playback.passes = 0;
            playback.finished = false;
        }
        let moving = speed.is_finite() && speed > 0.0;
        if !playing || !moving || !entry.visible || !lately || playback.finished {
            continue;
        }
        let before = playback.index;
        playback.elapsed += delta.mul_f64(speed.min(100.0));
        while playback.elapsed >= playback.delays[playback.index] {
            playback.elapsed -= playback.delays[playback.index];
            if playback.index + 1 < count {
                playback.index += 1;
                continue;
            }
            playback.passes += 1;
            if loops.is_some_and(|loops| playback.passes >= loops) {
                playback.finished = true;
                playback.elapsed = Duration::ZERO;
                break;
            }
            playback.index = 0;
        }
        let index = playback.index;
        if !playback.finished {
            let next = playback.delays[index]
                .saturating_sub(playback.elapsed)
                .div_f64(speed.min(100.0));
            quickest = Some(quickest.map_or(next, |quickest| quickest.min(next)));
        }
        if index != before {
            set(state, node, "frame", SceneValue::Number(index as f64));
            changed = true;
        }
    }
    // One deadline for all of them, at the pace of the quickest frame;
    // none while nothing plays.
    state.images.due = quickest.map(|next| now + next.clamp(FASTEST_WAKE, Duration::from_secs(1)));
    changed
}
