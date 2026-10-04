//! The configuration while the compositor offers no output at all.
//!
//! Every screen switched off, a dock unplugged, a compositor that removes an
//! output on DPMS: with no output there is no worker, and with no worker no
//! Lua runs -- not the timer that would light a screen again, not the IPC
//! verb that asks for it. A configuration that says
//! `morf.surface.outputless = true` is run here instead, once, in a runtime
//! with no surface: `morf.screens` is empty, nothing it declares is mapped,
//! and timers, IPC, D-Bus, file watches, processes and the compositor's
//! non-surface services (idle, clipboard, output power, gamma, toplevels)
//! work as ever. It holds a Wayland connection of its own and hears an output
//! arrive on it; the supervisor then stops it and starts the per-output
//! runtimes afresh, handing them what it kept with `morf.reloadable`.

use morf_lua::{Limits, Runtime};
use morf_app::{LayerClient, Event, Output};
use std::os::fd::AsFd;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::desktop::{desktop_for, dispatch_desktop};
use crate::{
    lock::*, paint::clock_text, services::apply_idle_timeouts, supervisor::execute_config_on,
    surface_layers::apply_service_requests, wake_plan::*, workers::*,
};

/// The name the outputless runtime goes by among the workers, in logs and in
/// `morf ipc capabilities`. Not a connector name any compositor uses.
pub(crate) const OUTPUTLESS: &str = "(no output)";

/// How often a runtime that could not reach the compositor tries again.
const RECONNECT: Duration = Duration::from_secs(1);

/// The screen the supervisor records for the outputless worker: never equal
/// to a real one, so a real output replaces it.
pub(crate) fn outputless_screen() -> Output {
    Output {
        name: Some(OUTPUTLESS.to_owned()),
        ..Output::default()
    }
}

/// Whether the configuration wants to run with no output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outputless {
    /// Nothing has run it yet (a shell started with every screen off): it is
    /// run to find out, and stopped at once if it says no.
    Unknown,
    Wanted,
    Unwanted,
}

impl Outputless {
    pub(crate) fn from_flag(wanted: bool) -> Self {
        if wanted { Self::Wanted } else { Self::Unwanted }
    }
}

/// Runs the configuration with no output until told to stop.
///
/// `connect` says whether to hold a Wayland connection: the shell does, to
/// hear outputs arrive and to reach the compositor's services; a test runs
/// without one.
pub(crate) fn run_outputless(start: WorkerStart, connect: bool) -> Result<(), String> {
    let (limits, warnings) = Limits::from_env();
    for warning in warnings {
        eprintln!("morf: {warning}");
    }
    let mut runtime = Runtime::new(limits);
    runtime.set_capabilities(&[("outputless".to_owned(), "true".to_owned())]);
    if let Some(seed) = start.seed.clone() {
        runtime.restore_reloadable_state(seed);
    }
    // Before the configuration runs, so it reads the answer from its first
    // line; the supervisor says when the duty moves later.
    runtime.set_primary(start.primary);
    let result = drive_outputless(&mut runtime, &start, connect);
    start.handover.deposit(&mut runtime);
    result
}

fn drive_outputless(
    runtime: &mut Runtime,
    start: &WorkerStart,
    connect: bool,
) -> Result<(), String> {
    let tx = &start.tx;
    let send = |message: WorkerMessage| {
        tx.send(SupervisorMessage::Worker(message))
            .map_err(|_| "output supervisor stopped".to_owned())
    };
    let loaded = execute_config_on(runtime, &start.path, &start.source, start.policy, &[]);
    let config = runtime.layer_surface_config();
    if let Err(error) = loaded {
        // A configuration written for a screen may well fail with none: that
        // is a configuration that cannot run here, not a shell that failed.
        runtime.warn(format!(
            "with no output the configuration did not load, so the shell waits for one: {error}"
        ));
        send(WorkerMessage::Loaded {
            output: OUTPUTLESS.to_owned(),
            session_lock: false,
            outputless: false,
        })?;
        return Ok(());
    }
    send(WorkerMessage::Loaded {
        output: OUTPUTLESS.to_owned(),
        session_lock: config.session_lock,
        outputless: config.outputless,
    })?;
    if config.session_lock {
        return crate::surface_run::await_lock(
            std::mem::take(runtime),
            &start.path,
            &start.stop,
            &start.commands,
        );
    }
    if !config.outputless {
        return Ok(());
    }
    let mut client = None;
    let mut desktop = None;
    let mut attempted: Option<Instant> = None;
    let mut clock = clock_text();
    runtime
        .update_clock(&clock)
        .map_err(|error| error.to_string())?;
    let wake = morf_io::Wake::new().map_err(|error| error.to_string())?;
    let mut follow_up = false;
    let mut pending_streak = 0;
    loop {
        if start.stop.load(Ordering::Acquire) {
            return Ok(());
        }
        if connect
            && client.is_none()
            && attempted.is_none_or(|at: Instant| at.elapsed() >= RECONNECT)
        {
            attempted = Some(Instant::now());
            client = connect_client(&send)?;
            desktop = client.as_ref().map(desktop_for).transpose()?;
            if let Some(desktop) = desktop.as_mut() {
                desktop.set_idle_timeouts(&runtime.idle_timeouts());
            }
        }
        let sleep = Sleep::plan_with(
            runtime,
            std::mem::take(&mut follow_up),
            None,
            &mut pending_streak,
        );
        let mut timeout = sleep.timeout();
        if connect && client.is_none() {
            timeout = Some(timeout.map_or(RECONNECT, |timeout| timeout.min(RECONNECT)));
        }
        let slept = Instant::now();
        match client.as_mut() {
            Some(client) => {
                let woke = client
                    .wait_for(timeout, Some(wake.as_fd()))
                    .map_err(|error| error.to_string())?;
                if let Some(desktop) = desktop.as_mut() {
                    dispatch_desktop(runtime, desktop, None)?;
                }
                log_wake(OUTPUTLESS, woke, &sleep, slept);
            }
            None => {
                wake.wait(timeout.unwrap_or(Duration::from_secs(3600)));
            }
        }
        wake.drain();
        let next_clock = clock_text();
        if next_clock != clock {
            clock = next_clock;
            runtime
                .update_clock(&clock)
                .map_err(|error| error.to_string())?;
        }
        runtime.poll_services();
        while let Ok(command) = start.commands.try_recv() {
            follow_up = true;
            let update = handle_worker_command(runtime, None, start.policy, command);
            if update.reloaded {
                send(WorkerMessage::Outputless {
                    output: OUTPUTLESS.to_owned(),
                    wanted: runtime.layer_surface_config().outputless,
                })?;
                if let Some(desktop) = desktop.as_mut() {
                    desktop.set_idle_timeouts(&runtime.idle_timeouts());
                    let _ = desktop.reset_gamma(None);
                }
            }
        }
        if let Some(hard) = runtime.take_reload_request() {
            tx.send(SupervisorMessage::Reload { hard })
                .map_err(|_| "output supervisor stopped".to_owned())?;
        }
        if runtime.quit_requested() {
            tx.send(SupervisorMessage::Quit)
                .map_err(|_| "output supervisor stopped".to_owned())?;
            return Ok(());
        }
        if let Some(enabled) = runtime.take_watch_files_change() {
            tx.send(SupervisorMessage::WatchFiles(enabled))
                .map_err(|_| "output supervisor stopped".to_owned())?;
        }
        // Declared and left unmapped: nothing here draws, so what a surface
        // change or a removed node would tell a renderer is dropped.
        runtime.take_layer_surface_change();
        runtime.take_window_surface_change();
        runtime.take_removed_nodes();
        let (Some(client), Some(desktop)) = (client.as_mut(), desktop.as_mut()) else {
            continue;
        };
        apply_service_requests(runtime, client, desktop);
        apply_idle_timeouts(runtime, desktop);
        while let Some(event) = client.next_event() {
            follow_up = true;
            match event {
                Event::Screens(screens) => send(WorkerMessage::Screens {
                    output: OUTPUTLESS.to_owned(),
                    screens,
                })?,
                Event::Clipboard { text } => {
                    runtime.dispatch_clipboard(text);
                }
                // Everything else belongs to a surface, and there is none.
                _ => {}
            }
        }
        apply_service_requests(runtime, client, desktop);
    }
}

/// A Wayland connection with no surface, which hears outputs arrive; `None`
/// while the compositor cannot be reached.
fn connect_client(
    send: &impl Fn(WorkerMessage) -> Result<(), String>,
) -> Result<Option<LayerClient>, String> {
    let Ok(mut client) = LayerClient::probe() else {
        return Ok(None);
    };
    client.set_waker(morf_io::wake_all);
    // An output may have come in the moment before this connection did.
    send(WorkerMessage::Screens {
        output: OUTPUTLESS.to_owned(),
        screens: client.screens().to_vec(),
    })?;
    Ok(Some(client))
}
