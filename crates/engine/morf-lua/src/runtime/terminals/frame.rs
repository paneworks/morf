//! The two places the loop meets the terminals: [`pump`] every turn and
//! [`sync`] every frame.

use super::*;

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
