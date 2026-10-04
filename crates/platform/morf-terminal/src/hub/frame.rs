//! The two places the loop meets the terminals: [`Hub::pump`] every turn
//! and [`Hub::fit`] every frame.

use super::*;

impl<H: Clone> Hub<H> {
    /// Feeds each terminal what its program wrote, up to [`FEED_PER_TURN`]
    /// bytes; answers the program's queries; notices exits, titles and bells.
    ///
    /// Returns the callbacks owed, whether any screen changed, and whether
    /// more is waiting for the next turn.
    pub fn pump(&mut self, scene: &mut Scene) -> (Vec<Call<H>>, bool, bool) {
        let mut calls = Vec::new();
        if self.entries.is_empty() {
            return (calls, false, false);
        }
        if let Some(reactor) = self.reactor.as_ref() {
            while let Some(event) = reactor.try_next() {
                let node = self.by_io.get(&event.id()).copied();
                // A terminal that is gone: its output is nobody's.
                if let Some(entry) = node.and_then(|node| self.entries.get_mut(&node)) {
                    entry.queue.push_back(event);
                }
            }
        }
        let now = Instant::now();
        let mut changed = false;
        let mut more = false;
        let nodes: Vec<NodeHandle> = self.entries.keys().copied().collect();
        for node in nodes {
            let mut exit = None;
            let events;
            // Whether anything reached the emulator this turn: a terminal
            // whose program said nothing is not looked at again.
            let mut fed = false;
            {
                let Some(entry) = self.entries.get_mut(&node) else {
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
                        self.by_io.remove(&io);
                    }
                    if let Some(mut pty) = entry.pty.take() {
                        pty.close();
                    }
                }
            }
            for event in events {
                let callbacks = &self.entries[&node].callbacks;
                let (callback, args) = match event {
                    TerminalEvent::Title(title) => {
                        let call = callbacks.on_title.clone();
                        let args = vec![IpcValue::String(title.clone())];
                        self.set_property(node, "title", SceneValue::String(title));
                        (call, args)
                    }
                    TerminalEvent::Bell => (callbacks.on_bell.clone(), Vec::new()),
                    TerminalEvent::Selection(text) => {
                        (callbacks.on_selection.clone(), vec![IpcValue::String(text)])
                    }
                    TerminalEvent::Clipboard(text) => {
                        (callbacks.on_clipboard.clone(), vec![IpcValue::String(text)])
                    }
                };
                if let Some(callback) = callback {
                    calls.push(Call { callback, args });
                }
            }
            if fed {
                changed |= self.refresh_screen(scene, node);
            }
            if let Some((code, signal)) = exit {
                self.set_property(node, "running", SceneValue::Bool(false));
                // A program ended by a signal exits, as a shell reports it,
                // with 128 and the signal's number.
                let status = code.or(signal.map(|signal| 128 + signal));
                self.set_property(
                    node,
                    "exit_code",
                    status.map_or(SceneValue::Nil, |status| {
                        SceneValue::Number(f64::from(status))
                    }),
                );
                if let Some(callback) = &self.entries[&node].callbacks.on_exit {
                    calls.push(Call {
                        callback: callback.clone(),
                        args: vec![
                            status.map_or(IpcValue::Nil, |status| {
                                IpcValue::Integer(i64::from(status))
                            }),
                            signal.map_or(IpcValue::Nil, |signal| {
                                IpcValue::Integer(i64::from(signal))
                            }),
                        ],
                    });
                }
                changed = true;
            }
        }
        (calls, changed, more)
    }
}

impl<H> Hub<H> {
    /// The nodes that have terminals.
    pub fn nodes(&self) -> Vec<NodeHandle> {
        self.entries.keys().copied().collect()
    }

    /// Fits a laid-out terminal's grid to its box of `size` pixels with the
    /// cell `metrics`, starting its program the first time; see the module
    /// docs. Returns whether anything changed.
    pub fn fit(
        &mut self,
        scene: &mut Scene,
        node: NodeHandle,
        size: (f64, f64),
        metrics: TerminalMetrics,
    ) -> bool {
        let Ok(padding) = scene.number(node, "padding") else {
            return false;
        };
        let padding = padding.max(0.0);
        let columns = ((size.0 - padding * 2.0) / metrics.cell_width)
            .floor()
            .clamp(2.0, 1000.0) as usize;
        let rows = ((size.1 - padding * 2.0) / metrics.cell_height)
            .floor()
            .clamp(1.0, 1000.0) as usize;
        let pty_size = PtySize {
            columns: columns as u16,
            rows: rows as u16,
            cell_width: metrics.cell_width as u16,
            cell_height: metrics.cell_height as u16,
        };
        let Some(entry) = self.entries.get_mut(&node) else {
            return false;
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
            match self.start(node, pty_size) {
                Ok(()) => self.set_property(node, "running", SceneValue::Bool(true)),
                Err(message) => {
                    // Said on the terminal itself, where whoever is looking
                    // at it will see it, and answered as a shell answers a
                    // program it cannot run: an exit of 127, on the next
                    // turn, through `on_exit`.
                    self.effects.warnings.push(format!("Terminal: {message}"));
                    if let Some(entry) = self.entries.get_mut(&node) {
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
        if scene.number(node, "columns").ok() != Some(columns as f64) {
            self.set_property(node, "columns", SceneValue::Number(columns as f64));
        }
        if scene.number(node, "rows").ok() != Some(rows as f64) {
            self.set_property(node, "rows", SceneValue::Number(rows as f64));
        }
        self.refresh_screen(scene, node)
    }
}
