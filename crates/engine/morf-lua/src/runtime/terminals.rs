//! `ui.Terminal`: a program on a pseudo-terminal, as a node.
//!
//! The runtime keeps one [`TerminalEntry`] per `Terminal` node: the emulator,
//! the pty once the program is started, and the reactor events waiting to
//! be fed. The pieces meet in three places:
//!
//! - [`TerminalHub::pump`], every turn of the loop: output the reactor read
//!   is fed to the emulator — at most [`FEED_PER_TURN`] bytes a terminal, so
//!   a program printing as fast as it can shares the loop with everything
//!   else — and a new picture of the screen is hung on the node when
//!   anything on it changed. A terminal with nothing to say costs a map
//!   lookup, and no frame.
//! - [`sync`], every frame, after layout: the node's size in cells follows
//!   its laid-out size and the cell the font makes, and a change of size
//!   reaches the program as a resize. The program is started here, the first
//!   time the node is laid out, so it starts at its real size.
//! - Keys, the pointer and the node's methods, which write to the program.
//!
//! The terminal's own reactor is separate from `morf.spawn`'s: the two
//! drain their events differently (a terminal's output is fed in slices,
//! credited back as it is), and neither need know about the other.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use morf_io::{IoEvent, IoId, Reactor};
use morf_layout::Layout;
use morf_scene::{Color, NodeHandle, TerminalMetrics, Value as SceneValue};
use morf_terminal::{
    Emulator, Modifiers, MouseAction, MouseButton, Palette, Pty, PtyOptions, PtySize, ScreenStyle,
    SelectionKind, TerminalEvent,
};
use morf_text::TextSystem;

use crate::scene_bindings::assign_scene_property;
use crate::state::ReactiveState;
use crate::surface_types::IpcValue;
use morf_runtime::Handler;

mod control;
mod frame;
mod input;

pub(crate) use control::{clear_selection, kill, paste, pid, scroll, selection, text, write};
pub(crate) use frame::{pump, sync};
pub(crate) use input::{key, pointer, set_focus, wheel};

/// Bytes of a program's output fed to its emulator per turn of the loop.
pub(crate) const FEED_PER_TURN: usize = 256 * 1024;
/// History kept unless the configuration says otherwise.
pub(crate) const DEFAULT_SCROLLBACK: usize = 10_000;
/// Lines one wheel step moves the history.
const WHEEL_LINES: i32 = 3;
/// Presses closer together than this on one cell count as one click more.
const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(400);

/// How to start a terminal's program.
pub(crate) struct TerminalSpec {
    pub(crate) command: Vec<String>,
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) working_directory: Option<PathBuf>,
    pub(crate) scrollback: usize,
}

/// A terminal's callbacks.
#[derive(Default)]
pub(crate) struct TerminalCallbacks {
    pub(crate) on_exit: Option<Handler>,
    pub(crate) on_title: Option<Handler>,
    pub(crate) on_bell: Option<Handler>,
    pub(crate) on_clipboard: Option<Handler>,
    pub(crate) on_selection: Option<Handler>,
}

pub(crate) struct TerminalEntry {
    spec: TerminalSpec,
    callbacks: TerminalCallbacks,
    emulator: Emulator,
    pty: Option<Pty>,
    /// The reactor's id for the pty, while it is being listened to.
    io: Option<IoId>,
    queue: VecDeque<IoEvent>,
    /// Typed or written before the program was started.
    pending: Vec<u8>,
    started: bool,
    exited: bool,
    /// The cell, once the node has been laid out.
    metrics: Option<TerminalMetrics>,
    /// A button held down on it, for drag reports.
    held: Option<MouseButton>,
    /// A wheel's pixels not yet a whole line.
    wheel_rest: f64,
    /// Selecting with the pointer: a drag is under way.
    selecting: bool,
    /// The last left press, for double and triple clicks: when, which
    /// cell, and how many in a row.
    last_click: Option<(Instant, (usize, usize), u8)>,
}

/// A callback owed.
pub(crate) struct TerminalCall {
    pub(crate) callback: Handler,
    pub(crate) args: Vec<IpcValue>,
}

/// The runtime's terminals.
#[derive(Default)]
pub(crate) struct TerminalHub {
    // Entries before the reactor: dropping an entry hangs its program up,
    // which is an order to the reactor, and the reactor must still be there
    // to take it before it shuts down and kills what is left.
    entries: HashMap<NodeHandle, TerminalEntry>,
    by_io: HashMap<IoId, NodeHandle>,
    /// The terminal the keyboard last went to.
    focused: Option<NodeHandle>,
    reactor: Option<Reactor>,
}

impl TerminalHub {
    /// When the earliest synchronized update a program left open must be
    /// shown anyway: the one thing a terminal owes the loop on a clock.
    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.entries
            .values()
            .filter_map(|entry| entry.emulator.sync_deadline())
            .min()
    }

    /// How many terminals there are.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn contains(&self, node: NodeHandle) -> bool {
        self.entries.contains_key(&node)
    }

    /// Adds a terminal for a node; its program starts when it is laid out.
    pub(crate) fn register(
        &mut self,
        node: NodeHandle,
        spec: TerminalSpec,
        callbacks: TerminalCallbacks,
    ) {
        let emulator = Emulator::new(80, 24, spec.scrollback);
        self.entries.insert(
            node,
            TerminalEntry {
                spec,
                callbacks,
                emulator,
                pty: None,
                io: None,
                queue: VecDeque::new(),
                pending: Vec::new(),
                started: false,
                exited: false,
                metrics: None,
                held: None,
                wheel_rest: 0.0,
                selecting: false,
                last_click: None,
            },
        );
    }

    /// Forgets a node's terminal, hanging its program up.
    pub(crate) fn remove(&mut self, node: NodeHandle) {
        if let Some(entry) = self.entries.remove(&node)
            && let Some(io) = entry.io
        {
            self.by_io.remove(&io);
        }
        if self.focused == Some(node) {
            self.focused = None;
        }
    }

    fn reactor(&mut self) -> std::io::Result<&Reactor> {
        if self.reactor.is_none() {
            self.reactor = Some(Reactor::new()?);
        }
        Ok(self.reactor.as_ref().expect("just made"))
    }

    fn start(&mut self, node: NodeHandle, size: PtySize) -> Result<(), String> {
        let Some(entry) = self.entries.get(&node) else {
            return Ok(());
        };
        let options = PtyOptions {
            command: entry.spec.command.clone(),
            environment: entry.spec.environment.clone(),
            working_directory: entry.spec.working_directory.clone(),
            size,
        };
        let reactor = self.reactor().map_err(|error| error.to_string())?;
        let pty = Pty::spawn(reactor, &options)
            .map_err(|error| format!("{}: {error}", options.command[0]))?;
        let io = pty.handle().id();
        let entry = self.entries.get_mut(&node).expect("present");
        let pending = std::mem::take(&mut entry.pending);
        if !pending.is_empty() {
            let _ = pty.write(pending);
        }
        entry.pty = Some(pty);
        entry.io = Some(io);
        self.by_io.insert(io, node);
        Ok(())
    }
}

impl TerminalEntry {
    /// Writes to the program, or keeps it for when it starts.
    fn write(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        if bytes.is_empty() {
            return Ok(());
        }
        match &self.pty {
            Some(pty) if !self.exited => pty.write(bytes).map_err(|error| error.to_string()),
            Some(_) => Err("the program has exited".into()),
            None if self.exited => Err("the program could not be started".into()),
            None => {
                if self.pending.len() + bytes.len() > morf_io::MAX_OUTGOING {
                    return Err("too much written before the program started".into());
                }
                self.pending.extend(bytes);
                Ok(())
            }
        }
    }
}

/// Reads a colour from a `colors` table entry.
fn rgba(value: &SceneValue) -> Option<[u8; 4]> {
    let color = match value {
        SceneValue::Color(color) => *color,
        SceneValue::String(text) => Color::parse(text)?,
        _ => return None,
    };
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some([
        channel(color.red),
        channel(color.green),
        channel(color.blue),
        channel(color.alpha),
    ])
}

/// The palette a node's `colors` asks for, on top of the default one.
fn palette_of(value: &SceneValue) -> Palette {
    let mut palette = Palette::default();
    let SceneValue::Map(map) = value else {
        return palette;
    };
    if let Some(color) = map.get("foreground").and_then(rgba) {
        palette.foreground = color;
    }
    if let Some(color) = map.get("background").and_then(rgba) {
        palette.background = color;
    }
    if let Some(color) = map.get("cursor").and_then(rgba) {
        palette.cursor = Some(color);
    }
    if let Some(color) = map.get("cursor_text").and_then(rgba) {
        palette.cursor_text = Some(color);
    }
    if let Some(SceneValue::List(colors)) = map.get("palette") {
        for (slot, color) in palette.ansi.iter_mut().zip(colors) {
            if let Some(color) = rgba(color) {
                *slot = color;
            }
        }
    }
    palette
}

/// How a node's screen is drawn, from its properties.
fn style_of(
    state: &ReactiveState,
    node: NodeHandle,
    metrics: TerminalMetrics,
    focused: bool,
) -> Option<ScreenStyle> {
    let scene = &state.scene;
    Some(ScreenStyle {
        palette: palette_of(scene.current(node, "colors").ok()?),
        font_family: scene.string_value(node, "font_family").ok()?.to_owned(),
        font_size: scene.number(node, "font_size").ok()?,
        padding: scene.number(node, "padding").ok()?.max(0.0),
        metrics,
        focused,
    })
}

fn is_focused(state: &ReactiveState, node: NodeHandle) -> bool {
    state.terminals.focused == Some(node) || state.scene.bool_value(node, "focus").unwrap_or(false)
}

/// Hangs a new picture of a terminal's screen on its node, when something
/// on it changed. Returns whether one was hung.
fn refresh_screen(state: &mut ReactiveState, node: NodeHandle) -> bool {
    let focused = is_focused(state, node);
    let Some(metrics) = state
        .terminals
        .entries
        .get(&node)
        .and_then(|entry| entry.metrics)
    else {
        return false;
    };
    let Some(style) = style_of(state, node, metrics, focused) else {
        return false;
    };
    let Some(entry) = state.terminals.entries.get_mut(&node) else {
        return false;
    };
    let Some(screen) = entry.emulator.screen(&style) else {
        return false;
    };
    state.scene.set_terminal_screen(node, Arc::new(screen));
    state.scene_revision = state.scene_revision.wrapping_add(1);
    true
}

fn set_property(state: &mut ReactiveState, node: NodeHandle, property: &str, value: SceneValue) {
    let _ = assign_scene_property(state, node, property, value);
}
