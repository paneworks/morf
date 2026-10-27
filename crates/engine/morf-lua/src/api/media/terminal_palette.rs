//! `morf.terminal`: a terminal of the shell's own that hears the colours a
//! colour tool (pywal, wallust, lule) writes to every terminal, and the same
//! sequences read from a file.
//!
//! ```lua
//! local tty = morf.terminal.listen(function(palette, changed) end)
//! tty.path         -- "/dev/pts/7": one of the terminals the tool writes to
//! tty:stop()
//! local palette = morf.terminal.parse(morf.fs.read("~/.cache/wal/sequences"))
//! ```
//!
//! A palette is `{ colors = { [1] = colour 0, ... [256] = colour 255 },
//! foreground, background, cursor }` as `morf.color` values, each nil until
//! something set it. A listener's palette gathers every sequence it has
//! heard, so each call has the whole of it; `changed` lists what moved in
//! that burst (0-255, "foreground", "background", "cursor").

use crate::value::ipc::IpcToLua;
use std::collections::BTreeMap;
use std::sync::Arc;

use luna::{Callback, CallbackReturn, Context, Table};
use morf_terminal::palette::{BACKGROUND, CURSOR, ColorChange, FOREGROUND, PaletteTty};

use crate::api::ui::scene::HostError;
use crate::runtime::host_types::IpcValue;
use crate::runtime::state::ReactiveState;
use crate::value::ipc::IpcTable;
use morf_runtime::Handler;

/// The most listeners a runtime keeps: each is a terminal and a thread.
const MAX_LISTENERS: usize = 8;

struct PaletteListener {
    id: u64,
    tty: PaletteTty,
    callback: Handler,
    colors: Vec<Option<[u8; 3]>>,
}

/// Every `morf.terminal.listen` terminal, and the id the next one gets.
#[derive(Default)]
pub(crate) struct PaletteListeners {
    listeners: Vec<PaletteListener>,
    next: u64,
}

impl PaletteListeners {
    pub(crate) fn len(&self) -> usize {
        self.listeners.len()
    }

    /// Listens on `tty` for `callback`. Returns the id `stop` takes.
    pub(crate) fn listen(&mut self, tty: PaletteTty, callback: Handler) -> u64 {
        let id = self.next;
        self.next += 1;
        self.listeners.push(PaletteListener {
            id,
            tty,
            callback,
            colors: vec![None; 259],
        });
        id
    }

    pub(crate) fn stop(&mut self, id: u64) {
        self.listeners.retain(|listener| listener.id != id);
    }

    /// Each listener that heard colours since the last turn: its callback,
    /// with the whole palette and what moved.
    pub(crate) fn heard(&mut self) -> Vec<(Handler, Vec<IpcValue>)> {
        let mut calls = Vec::new();
        for listener in &mut self.listeners {
            let changes = listener.tty.take();
            if changes.is_empty() {
                continue;
            }
            apply(&mut listener.colors, &changes);
            if morf_value::env::log("palette") {
                eprintln!(
                    "morf: {} heard {} colour(s)",
                    listener.tty.path().display(),
                    changes.len()
                );
            }
            calls.push((
                listener.callback.clone(),
                vec![palette_value(&listener.colors), changed_value(&changes)],
            ));
        }
        calls
    }
}

fn color(rgb: Option<[u8; 3]>) -> IpcValue {
    match rgb {
        Some([r, g, b]) => IpcValue::Color(morf_scene::Color::rgba8(r, g, b, 255)),
        None => IpcValue::Nil,
    }
}

fn index_name(index: usize) -> IpcValue {
    match index {
        FOREGROUND => IpcValue::String("foreground".to_owned()),
        BACKGROUND => IpcValue::String("background".to_owned()),
        CURSOR => IpcValue::String("cursor".to_owned()),
        other => IpcValue::Integer(other as i64),
    }
}

/// Folds changes into a palette of 259 slots.
pub(crate) fn apply(colors: &mut [Option<[u8; 3]>], changes: &[ColorChange]) {
    for (index, rgb) in changes {
        if let Some(slot) = colors.get_mut(*index) {
            *slot = *rgb;
        }
    }
}

/// A palette as Lua sees it.
pub(crate) fn palette_value(colors: &[Option<[u8; 3]>]) -> IpcValue {
    let list = (0..256)
        .map(|index| color(colors.get(index).copied().flatten()))
        .collect::<Vec<_>>();
    let mut map = BTreeMap::new();
    map.insert(
        "colors".to_owned(),
        IpcValue::Table(Arc::new(IpcTable::List(list))),
    );
    for (name, index) in [
        ("foreground", FOREGROUND),
        ("background", BACKGROUND),
        ("cursor", CURSOR),
    ] {
        map.insert(name.to_owned(), color(colors.get(index).copied().flatten()));
    }
    IpcValue::Table(Arc::new(IpcTable::Map(map)))
}

/// What changed in a burst, as Lua sees it.
pub(crate) fn changed_value(changes: &[ColorChange]) -> IpcValue {
    let mut seen = Vec::new();
    for (index, _) in changes {
        if !seen.contains(index) {
            seen.push(*index);
        }
    }
    IpcValue::Table(Arc::new(IpcTable::List(
        seen.into_iter().map(index_name).collect(),
    )))
}

pub(crate) fn install_palette_api<'gc>(
    ctx: Context<'gc>,
    state: std::rc::Rc<crate::runtime::state::StateCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let terminal = Table::new(&ctx);
    let listen_state = std::rc::Rc::clone(&state);
    terminal.set_field(
        ctx,
        "listen",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let callback: luna::Closure = stack.consume(ctx)?;
            let mut state = listen_state.borrow_mut();
            if state.palettes().len() >= MAX_LISTENERS {
                return Err(HostError(format!(
                    "morf.terminal.listen keeps at most {MAX_LISTENERS} terminals"
                ))
                .into());
            }
            let tty = PaletteTty::open()
                .map_err(|error| HostError(format!("no terminal to listen on: {error}")))?;
            let handle = Table::new(&ctx);
            handle.set_field(
                ctx,
                "path",
                ctx.intern(tty.path().to_string_lossy().as_bytes()),
            );
            let callback = crate::vm::handler_store::register(ctx.stash(callback));
            let id = state.palettes_mut().listen(tty, callback);
            let stop_state = std::rc::Rc::clone(&listen_state);
            handle.set_field(
                ctx,
                "stop",
                Callback::from_fn(&ctx, move |_, _, _| {
                    stop_state.borrow_mut().palettes_mut().stop(id);
                    Ok(CallbackReturn::Return)
                }),
            );
            stack.replace(ctx, handle);
            Ok(CallbackReturn::Return)
        }),
    );
    terminal.set_field(
        ctx,
        "parse",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let text: luna::Value = stack.consume(ctx)?;
            let bytes = match text {
                luna::Value::String(text) => text.as_bytes().to_vec(),
                luna::Value::Nil => Vec::new(),
                _ => return Err(HostError("morf.terminal.parse takes text".into()).into()),
            };
            let mut colors = vec![None; 259];
            apply(
                &mut colors,
                &morf_terminal::palette::parse_color_sequences(&bytes),
            );
            stack.replace(ctx, palette_value(&colors).to_lua(ctx));
            Ok(CallbackReturn::Return)
        }),
    );
    morf.set_field(ctx, "terminal", terminal);
}

impl crate::Runtime {
    /// Hands each `morf.terminal.listen` callback the colours its terminal
    /// heard since the last turn. True when any did.
    pub(crate) fn poll_palette_listeners(&mut self) -> bool {
        let calls = self.reactive.borrow_mut().palettes_mut().heard();
        let any = !calls.is_empty();
        for (callback, args) in calls {
            if let Err(message) = self.run_handler(|ctx, limits| {
                crate::vm::execute::execute_handler_args(ctx, &callback, &args, limits)
            }) {
                self.reactive.borrow_mut().log(
                    crate::LogLevel::Warn,
                    format!("morf.terminal.listen: {message}"),
                );
            }
        }
        any
    }
}
