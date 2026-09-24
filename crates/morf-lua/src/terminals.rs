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

use luna::StashedClosure;
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
    pub(crate) on_exit: Option<StashedClosure>,
    pub(crate) on_title: Option<StashedClosure>,
    pub(crate) on_bell: Option<StashedClosure>,
    pub(crate) on_clipboard: Option<StashedClosure>,
    pub(crate) on_selection: Option<StashedClosure>,
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
    pub(crate) callback: StashedClosure,
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

/// Feeds each terminal what its program wrote, up to [`FEED_PER_TURN`]
/// bytes; answers the program's queries; notices exits, titles and bells.
///
/// Returns the callbacks owed, whether any screen changed, and whether more
/// is waiting for the next turn.
pub(crate) fn pump(state: &mut ReactiveState) -> (Vec<TerminalCall>, bool, bool) {
    let mut calls = Vec::new();
    if state.terminals.entries.is_empty() {
        return (calls, false, false);
    }
    if let Some(reactor) = state.terminals.reactor.as_ref() {
        while let Some(event) = reactor.try_next() {
            let node = state.terminals.by_io.get(&event.id()).copied();
            // A terminal that is gone: its output is nobody's.
            if let Some(entry) = node.and_then(|node| state.terminals.entries.get_mut(&node)) {
                entry.queue.push_back(event);
            }
        }
    }
    let now = Instant::now();
    let mut changed = false;
    let mut more = false;
    let nodes: Vec<NodeHandle> = state.terminals.entries.keys().copied().collect();
    for node in nodes {
        let mut exit = None;
        let events;
        // Whether anything reached the emulator this turn: a terminal whose
        // program said nothing is not looked at again.
        let mut fed = false;
        {
            let Some(entry) = state.terminals.entries.get_mut(&node) else {
                continue;
            };
            let mut budget = FEED_PER_TURN;
            while budget > 0 {
                let Some(event) = entry.queue.pop_front() else {
                    break;
                };
                let weight = event.weight();
                fed = true;
                match event {
                    IoEvent::Stdout(_, bytes) => {
                        budget = budget.saturating_sub(bytes.len());
                        entry.emulator.feed(&bytes);
                    }
                    IoEvent::Exit { code, signal, .. } => exit = Some((code, signal)),
                    _ => {}
                }
                if let Some(pty) = &entry.pty {
                    pty.credit(weight);
                }
            }
            more |= !entry.queue.is_empty();
            fed |= entry.emulator.expire_sync(now);
            let replies = entry.emulator.take_replies();
            if !replies.is_empty() && !entry.exited {
                let _ = entry.write(replies);
            }
            events = entry.emulator.take_events();
            if exit.is_some() {
                entry.exited = true;
                if let Some(io) = entry.io.take() {
                    state.terminals.by_io.remove(&io);
                }
                if let Some(mut pty) = entry.pty.take() {
                    pty.close();
                }
            }
        }
        for event in events {
            let entry = &state.terminals.entries[&node];
            match event {
                TerminalEvent::Title(title) => {
                    if let Some(callback) = &entry.callbacks.on_title {
                        calls.push(TerminalCall {
                            callback: callback.clone(),
                            args: vec![IpcValue::String(title.clone())],
                        });
                    }
                    set_property(state, node, "title", SceneValue::String(title));
                }
                TerminalEvent::Bell => {
                    if let Some(callback) = &entry.callbacks.on_bell {
                        calls.push(TerminalCall {
                            callback: callback.clone(),
                            args: Vec::new(),
                        });
                    }
                }
                TerminalEvent::Selection(text) => {
                    if let Some(callback) = &entry.callbacks.on_selection {
                        calls.push(TerminalCall {
                            callback: callback.clone(),
                            args: vec![IpcValue::String(text)],
                        });
                    }
                }
                TerminalEvent::Clipboard(text) => {
                    if let Some(callback) = &entry.callbacks.on_clipboard {
                        calls.push(TerminalCall {
                            callback: callback.clone(),
                            args: vec![IpcValue::String(text)],
                        });
                    }
                }
            }
        }
        if fed {
            changed |= refresh_screen(state, node);
        }
        if let Some((code, signal)) = exit {
            set_property(state, node, "running", SceneValue::Bool(false));
            // A program ended by a signal exits, as a shell reports it, with
            // 128 and the signal's number.
            let status = code.or(signal.map(|signal| 128 + signal));
            set_property(
                state,
                node,
                "exit_code",
                status.map_or(SceneValue::Nil, |status| {
                    SceneValue::Number(f64::from(status))
                }),
            );
            if let Some(callback) = &state.terminals.entries[&node].callbacks.on_exit {
                calls.push(TerminalCall {
                    callback: callback.clone(),
                    args: vec![
                        status.map_or(IpcValue::Nil, |status| IpcValue::Integer(i64::from(status))),
                        signal.map_or(IpcValue::Nil, |signal| IpcValue::Integer(i64::from(signal))),
                    ],
                });
            }
            changed = true;
        }
    }
    (calls, changed, more)
}

/// Fits each laid-out terminal's grid to its box, starting its program the
/// first time; see the module docs. Returns whether anything changed.
pub(crate) fn sync(state: &mut ReactiveState, layout: &Layout, text: &mut TextSystem) -> bool {
    let mut changed = false;
    let nodes: Vec<NodeHandle> = state.terminals.entries.keys().copied().collect();
    for node in nodes {
        let Some(geometry) = layout.geometry(node) else {
            continue;
        };
        let (Ok(family), Ok(size), Ok(padding)) = (
            state
                .scene
                .string_value(node, "font_family")
                .map(str::to_owned),
            state.scene.number(node, "font_size"),
            state.scene.number(node, "padding"),
        ) else {
            continue;
        };
        let metrics = text.terminal_metrics(&family, size);
        let padding = padding.max(0.0);
        let columns = ((geometry.width - padding * 2.0) / metrics.cell_width)
            .floor()
            .clamp(2.0, 1000.0) as usize;
        let rows = ((geometry.height - padding * 2.0) / metrics.cell_height)
            .floor()
            .clamp(1.0, 1000.0) as usize;
        let pty_size = PtySize {
            columns: columns as u16,
            rows: rows as u16,
            cell_width: metrics.cell_width as u16,
            cell_height: metrics.cell_height as u16,
        };
        let Some(entry) = state.terminals.entries.get_mut(&node) else {
            continue;
        };
        let resized = entry.emulator.columns() != columns || entry.emulator.rows() != rows;
        let restyled = entry.metrics != Some(metrics);
        entry.metrics = Some(metrics);
        if resized || restyled {
            entry
                .emulator
                .resize(columns, rows, (pty_size.cell_width, pty_size.cell_height));
            if let Some(pty) = &entry.pty {
                let _ = pty.resize(pty_size);
            }
        }
        if !entry.started {
            entry.started = true;
            match state.terminals.start(node, pty_size) {
                Ok(()) => set_property(state, node, "running", SceneValue::Bool(true)),
                Err(message) => {
                    // Said on the terminal itself, where whoever is looking
                    // at it will see it, and answered as a shell answers a
                    // program it cannot run: an exit of 127, on the next
                    // turn, through `on_exit`.
                    state.log(crate::LogLevel::Warn, format!("Terminal: {message}"));
                    if let Some(entry) = state.terminals.entries.get_mut(&node) {
                        let line = format!("{message}\r\n");
                        entry.emulator.feed(line.as_bytes());
                        entry.queue.push_back(IoEvent::Exit {
                            id: 0,
                            code: Some(127),
                            signal: None,
                            timed_out: false,
                            truncated: false,
                        });
                        morf_io::wake_all();
                    }
                }
            }
        }
        if state.scene.number(node, "columns").ok() != Some(columns as f64) {
            set_property(state, node, "columns", SceneValue::Number(columns as f64));
        }
        if state.scene.number(node, "rows").ok() != Some(rows as f64) {
            set_property(state, node, "rows", SceneValue::Number(rows as f64));
        }
        changed |= refresh_screen(state, node);
    }
    changed
}

/// Sends one key press to a terminal's program. Returns whether the
/// terminal's picture changed (its cursor now solid, its view back at the
/// bottom of the history).
pub(crate) fn key(
    state: &mut ReactiveState,
    node: NodeHandle,
    keysym: u32,
    text: Option<&str>,
    modifiers: Modifiers,
) -> bool {
    let focus_moved = state.terminals.focused != Some(node);
    let previous = state.terminals.focused.replace(node);
    let Some(entry) = state.terminals.entries.get_mut(&node) else {
        return false;
    };
    let mut changed = false;
    if let Some(bytes) = entry.emulator.encode_key(keysym, text, modifiers) {
        // Typing is about now: back to where the program is writing.
        changed |= entry.emulator.scroll_to_bottom();
        let _ = entry.write(bytes);
    }
    if focus_moved {
        if let Some(previous) = previous {
            refresh_screen(state, previous);
        }
        changed = true;
    }
    changed | refresh_screen(state, node)
}

/// Where a point inside the node is on its grid.
fn cell_at(state: &ReactiveState, node: NodeHandle, local: (f64, f64)) -> Option<(usize, usize)> {
    cell_and_half_at(state, node, local).map(|(column, row, _)| (column, row))
}

/// The cell under a point, and whether the point is on its right half.
fn cell_and_half_at(
    state: &ReactiveState,
    node: NodeHandle,
    local: (f64, f64),
) -> Option<(usize, usize, bool)> {
    let entry = state.terminals.entries.get(&node)?;
    let metrics = entry.metrics?;
    let padding = state.scene.number(node, "padding").ok()?.max(0.0);
    let across = ((local.0 - padding) / metrics.cell_width).max(0.0);
    let row = ((local.1 - padding) / metrics.cell_height).floor().max(0.0) as usize;
    Some((
        (across.floor() as usize).min(entry.emulator.columns().saturating_sub(1)),
        row.min(entry.emulator.rows().saturating_sub(1)),
        across.fract() >= 0.5,
    ))
}

/// A pointer press, release or motion over a terminal: to the program when
/// it asked for the pointer. A press also gives the terminal the keyboard.
pub(crate) fn pointer(
    state: &mut ReactiveState,
    node: NodeHandle,
    action: MouseAction,
    button: Option<u32>,
    local: (f64, f64),
) -> bool {
    let mut changed = false;
    if action == MouseAction::Press && state.terminals.focused != Some(node) {
        let previous = state.terminals.focused.replace(node);
        if let Some(previous) = previous {
            refresh_screen(state, previous);
        }
        changed = true;
    }
    let Some((column, row, right_half)) = cell_and_half_at(state, node, local) else {
        return changed;
    };
    let Some(entry) = state.terminals.entries.get_mut(&node) else {
        return changed;
    };
    // A program that did not ask for the pointer leaves the left button to
    // selecting: a drag selects cells, a double click a word, a triple
    // click a line.
    let left = button.is_none_or(|code| MouseButton::from_code(code) == Some(MouseButton::Left));
    if !entry.emulator.mouse_modes().any() && (entry.selecting || left) {
        match action {
            MouseAction::Press if left => {
                let now = Instant::now();
                let count = match entry.last_click {
                    Some((at, cell, count))
                        if cell == (column, row) && now.duration_since(at) < DOUBLE_CLICK =>
                    {
                        count % 3 + 1
                    }
                    _ => 1,
                };
                entry.last_click = Some((now, (column, row), count));
                let kind = match count {
                    1 => SelectionKind::Cells,
                    2 => SelectionKind::Word,
                    _ => SelectionKind::Line,
                };
                entry.emulator.select_start(column, row, right_half, kind);
                entry.selecting = true;
            }
            MouseAction::Motion if entry.selecting => {
                entry.emulator.select_update(column, row, right_half);
            }
            MouseAction::Release if entry.selecting => {
                entry.selecting = false;
                entry.emulator.select_finish();
            }
            _ => return changed,
        }
        return changed | refresh_screen(state, node);
    }
    let button = match action {
        MouseAction::Press => {
            let pressed = button
                .and_then(MouseButton::from_code)
                .unwrap_or(MouseButton::Left);
            entry.held = Some(pressed);
            pressed
        }
        MouseAction::Release => {
            let released = button
                .and_then(MouseButton::from_code)
                .or(entry.held)
                .unwrap_or(MouseButton::Left);
            entry.held = None;
            released
        }
        MouseAction::Motion => entry.held.unwrap_or(MouseButton::None),
    };
    let held = entry.held.is_some();
    if let Some(bytes) =
        entry
            .emulator
            .encode_mouse(button, action, column, row, Modifiers::default(), held)
    {
        let _ = entry.write(bytes);
    }
    changed | refresh_screen(state, node)
}

/// A wheel turn over a terminal: to the program when it asked for the
/// pointer, as arrow keys to a full-screen program that did not, and
/// otherwise through the history.
pub(crate) fn wheel(
    state: &mut ReactiveState,
    node: NodeHandle,
    local: (f64, f64),
    pixels: f64,
    steps: i32,
) -> bool {
    let cell = cell_at(state, node, local);
    let Some(entry) = state.terminals.entries.get_mut(&node) else {
        return false;
    };
    // Whole lines: a wheel's detents three at a time, a touchpad's pixels
    // a cell's height at a time.
    let lines = if steps != 0 {
        steps * WHEEL_LINES
    } else {
        let height = entry.metrics.map_or(16.0, |metrics| metrics.cell_height);
        entry.wheel_rest += pixels;
        let whole = (entry.wheel_rest / height).trunc();
        entry.wheel_rest -= whole * height;
        whole as i32
    };
    if lines == 0 {
        return false;
    }
    let (column, row) = cell.unwrap_or((0, 0));
    if entry.emulator.mouse_modes().any() {
        let button = if lines < 0 {
            MouseButton::WheelUp
        } else {
            MouseButton::WheelDown
        };
        let mut bytes = Vec::new();
        for _ in 0..lines.unsigned_abs().min(12) {
            if let Some(report) = entry.emulator.encode_mouse(
                button,
                MouseAction::Press,
                column,
                row,
                Modifiers::default(),
                false,
            ) {
                bytes.extend(report);
            }
        }
        let _ = entry.write(bytes);
        return false;
    }
    if entry.emulator.alternate_scroll() {
        let keysym = if lines < 0 { 0xff52 } else { 0xff54 };
        let mut bytes = Vec::new();
        for _ in 0..lines.unsigned_abs().min(24) {
            if let Some(key) = entry
                .emulator
                .encode_key(keysym, None, Modifiers::default())
            {
                bytes.extend(key);
            }
        }
        let _ = entry.write(bytes);
        return false;
    }
    // Down the page is towards the bottom of the history.
    entry.emulator.scroll(-lines) && refresh_screen(state, node)
}

/// Gives the keyboard to a terminal, or takes it from whichever had it.
/// Returns whether that changed what a terminal's cursor looks like.
pub(crate) fn set_focus(state: &mut ReactiveState, node: Option<NodeHandle>) -> bool {
    if state.terminals.focused == node {
        return false;
    }
    let previous = std::mem::replace(&mut state.terminals.focused, node);
    let mut changed = false;
    for node in [previous, node].into_iter().flatten() {
        changed |= refresh_screen(state, node);
    }
    changed
}

/// Writes to a terminal's program.
pub(crate) fn write(
    state: &mut ReactiveState,
    node: NodeHandle,
    bytes: Vec<u8>,
) -> Result<(), String> {
    let entry = state
        .terminals
        .entries
        .get_mut(&node)
        .ok_or_else(|| "not a terminal".to_owned())?;
    entry.write(bytes)
}

/// Pastes text into a terminal, bracketed when its program asked for that.
pub(crate) fn paste(state: &mut ReactiveState, node: NodeHandle, text: &str) -> Result<(), String> {
    let entry = state
        .terminals
        .entries
        .get_mut(&node)
        .ok_or_else(|| "not a terminal".to_owned())?;
    entry.emulator.scroll_to_bottom();
    let bytes = entry.emulator.encode_paste(text);
    entry.write(bytes)
}

/// Signals a terminal's program. Returns whether it was running.
pub(crate) fn kill(state: &mut ReactiveState, node: NodeHandle, signal: i32) -> bool {
    match state.terminals.entries.get(&node) {
        Some(TerminalEntry {
            pty: Some(pty),
            exited: false,
            ..
        }) => {
            pty.signal(signal);
            true
        }
        _ => false,
    }
}

/// Moves a terminal's view through its history. Returns whether it moved.
pub(crate) fn scroll(state: &mut ReactiveState, node: NodeHandle, lines: i32) -> bool {
    let Some(entry) = state.terminals.entries.get_mut(&node) else {
        return false;
    };
    let moved = if lines == 0 {
        entry.emulator.scroll_to_bottom()
    } else {
        entry.emulator.scroll(lines)
    };
    if moved {
        refresh_screen(state, node);
    }
    moved
}

/// What a terminal shows, as text.
pub(crate) fn text(state: &ReactiveState, node: NodeHandle) -> Option<String> {
    Some(state.terminals.entries.get(&node)?.emulator.text())
}

/// The text selected with the pointer, if any.
pub(crate) fn selection(state: &ReactiveState, node: NodeHandle) -> Option<String> {
    state
        .terminals
        .entries
        .get(&node)?
        .emulator
        .selection_text()
}

/// Drops the selection. Whether there was one.
pub(crate) fn clear_selection(state: &mut ReactiveState, node: NodeHandle) -> bool {
    let Some(entry) = state.terminals.entries.get_mut(&node) else {
        return false;
    };
    let had = entry.emulator.select_clear();
    if had {
        refresh_screen(state, node);
    }
    had
}

/// The process id of a terminal's program, while it runs.
pub(crate) fn pid(state: &ReactiveState, node: NodeHandle) -> Option<u32> {
    let entry = state.terminals.entries.get(&node)?;
    if entry.exited {
        return None;
    }
    entry.pty.as_ref()?.pid()
}
