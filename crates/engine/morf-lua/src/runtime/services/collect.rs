//! What a turn collects from the services while the state is borrowed: PAM
//! verdicts, due timers, bus traffic, udev, status notifiers, HTTP answers,
//! I/O and watch callbacks, and transform callbacks.

use super::*;

/// Takes the PAM authentications that have finished.
pub(super) fn collect_pam_results(state: &mut ReactiveState, collected: &mut Collected) {
    collected.ready.extend(state.auth.take_verdicts());
}

/// Takes what every other source has for this turn.
pub(super) fn drain(state: &mut ReactiveState, collected: &mut Collected) {
    let Collected {
        timers,
        dbus_signals,
        dbus_replies,
        dbus_calls,
        pam_messages,
        greetd_messages,
        udev_events,
        status_updates,
        http_answers,
        io_calls,
        io_more,
        watch_calls,
        watch_more,
        retained_destroys,
        transform_callbacks,
        ..
    } = collected;
    for timer in state.timers_mut().collect_due() {
        if wake_log_wanted() {
            eprintln!(
                "{} morf: timer {} fired ({:.0} ms{})",
                crate::profile::stamp(),
                timer.origin,
                timer.interval.as_secs_f64() * 1000.0,
                if timer.repeat { ", repeating" } else { "" }
            );
        }
        // A one-shot `Timer` node has run its course.
        if !timer.repeat
            && let Some(node) = timer.node
        {
            let _ = assign_scene_property(state, node, "running", SceneValue::Bool(false));
        }
        timers.push(timer);
    }
    dbus_signals.extend(state.dbus.take_signals());
    dbus_replies.extend(state.dbus.take_replies());
    // A conversation says a few things per turn and then waits on a
    // person, so this never runs long.
    pam_messages.extend(state.auth.take_session_messages());
    greetd_messages.extend(state.auth.take_greetd_messages());
    // Bounded per frame (`morf_io::MAX_CALLS_PER_FRAME`): the caller is
    // blocked until we answer.
    dbus_calls.extend(state.dbus.drain_calls());
    let (events, udev_errors) = state.feeds.take_device_events();
    udev_events.extend(events);
    for error in udev_errors {
        state.log(LogLevel::Warn, format!("udev: {error}"));
    }
    let (updates, status_errors) = state.feeds.take_tray_updates();
    status_updates.extend(updates);
    for error in status_errors {
        state.log(LogLevel::Warn, format!("status notifier: {error}"));
    }
    // A finished request's answer is delivered once the state is released.
    http_answers.extend(state.feeds.take_http_answers());
    (*io_calls, *io_more) = state.io.collect();
    (*watch_calls, *watch_more) = state.watches.collect();
    retained_destroys.extend(state.retained_mut().retained_destroy_queue.drain());
    transform_callbacks.extend(state.transform_watchers.take_pending());
}
