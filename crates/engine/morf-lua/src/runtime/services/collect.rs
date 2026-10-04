//! What a turn collects from the services while the state is borrowed: PAM
//! verdicts, due timers, bus traffic, udev, status notifiers, HTTP answers,
//! I/O and watch callbacks, and transform callbacks.

use super::*;

/// Takes the PAM authentications that have finished.
pub(super) fn collect_pam_results(state: &mut ReactiveState, collected: &mut Collected) {
    let ready = &mut collected.ready;
    let mut index = 0;
    while index < state.pam_tasks.len() {
        let result = state.pam_tasks[index].task.wait(Duration::ZERO);
        if let Some(result) = result {
            let task = state.pam_tasks.swap_remove(index);
            ready.push((task.callback, task.unlock_on_success, result));
        } else {
            index += 1;
        }
    }
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
    for timer in state.timers.collect_due() {
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
    for subscription in &state.dbus_signals {
        while let Some(event) = subscription.signal.next_event(Duration::ZERO) {
            dbus_signals.push((
                subscription.id,
                subscription.callback.clone(),
                subscription.kind,
                event,
            ));
        }
    }
    // An answer leaves the list as it is delivered, and so does one
    // that missed its deadline; the rest wait for a later turn.
    state
        .dbus_replies
        .retain(|entry| match entry.reply.try_take() {
            Some(reply) => {
                dbus_replies.push((entry.callback.clone(), reply));
                false
            }
            None => true,
        });
    // A conversation says a few things per turn and then waits on a
    // person, so this never runs long. A finished session leaves the
    // list after its verdict is delivered, which is why the verdict is
    // collected first and the removal follows it.
    state.pam_sessions.retain(|entry| {
        let mut finished = false;
        for _ in 0..8 {
            let Some(event) = entry.session.borrow_mut().next(Duration::ZERO) else {
                break;
            };
            finished |= matches!(event, PamEvent::Finished(_));
            pam_messages.push((entry.callback.clone(), event));
            if finished {
                break;
            }
        }
        !finished
    });
    // The same for a greetd login: a few replies per turn, then a
    // wait on greetd, or on a person greetd is waiting on.
    state.greetd_sessions.retain(|entry| {
        let mut conversation = entry.conversation.borrow_mut();
        for _ in 0..8 {
            let Some(event) = conversation.next(Duration::ZERO) else {
                break;
            };
            let last = matches!(event, GreetdEvent::Failed(_));
            greetd_messages.push((entry.callback.clone(), event));
            if last {
                break;
            }
        }
        !conversation.ended()
    });
    // Bounded per frame (`morf_io::MAX_CALLS_PER_FRAME`): the caller is
    // blocked until we answer.
    dbus_calls.extend(state.dbus_services.drain_calls());
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
            morf_io::wake_all();
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
    retained_destroys.extend(state.retained.retained_destroy_queue.drain());
    for watcher in state.transform_watchers.values_mut() {
        if watcher.pending {
            watcher.pending = false;
            if let Some(callback) = &watcher.callback {
                transform_callbacks.push((callback.clone(), watcher.revision));
            }
        }
    }
}
