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
    let mut udev_errors = Vec::new();
    for subscription in &mut state.udev_monitors {
        let mut drained = false;
        for _ in 0..32 {
            match subscription.monitor.next_event(Duration::ZERO) {
                Ok(Some(event)) => {
                    udev_events.push((subscription.callback.clone(), event));
                }
                Ok(None) => {
                    drained = true;
                    break;
                }
                Err(error) => {
                    udev_errors.push(error.to_string());
                    drained = true;
                    break;
                }
            }
        }
        // This turn's share is taken; the monitor's alarm only rings
        // again once a drain finds the socket empty, so the rest is
        // asked for now.
        if !drained {
            morf_io::waker().wake();
        }
    }
    for error in udev_errors {
        state.log(LogLevel::Warn, format!("udev: {error}"));
    }
    let mut status_errors = Vec::new();
    for subscription in &mut state.status_notifiers {
        match subscription.host.poll_changed() {
            Ok(Some(items)) => status_updates.push((subscription.callback.clone(), items)),
            Ok(None) => {}
            Err(error) => status_errors.push(error.to_string()),
        }
    }
    for error in status_errors {
        state.log(LogLevel::Warn, format!("status notifier: {error}"));
    }
    // A cancelled request leaves here without a word, and dropping
    // its task stops the worker; a finished one leaves with its
    // answer, which is delivered below once the state is released.
    state.http_requests.retain_mut(|entry| {
        if entry.handle.cancelled.get() {
            return false;
        }
        let Some(outcome) = entry.task.poll() else {
            return true;
        };
        if let Some(callback) = entry.callback.take() {
            http_answers.push((
                callback,
                outcome,
                std::mem::take(&mut entry.url),
                entry.json.clone(),
                std::rc::Rc::clone(&entry.handle),
            ));
        } else {
            entry.handle.done.set(true);
        }
        false
    });
    (*io_calls, *io_more) = state.io.collect();
    (*watch_calls, *watch_more) = state.watches.collect();
    retained_destroys.extend(state.retained_mut().retained_destroy_queue.drain());
    transform_callbacks.extend(state.transform_watchers.take_pending());
}
