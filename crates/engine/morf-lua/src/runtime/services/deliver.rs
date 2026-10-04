//! The callbacks a turn collected, run once the state is let go, each with
//! bounded fuel.

use super::*;

impl Runtime {
    /// Runs what `collected` holds, in the order the services are polled.
    pub(super) fn deliver(&mut self, collected: Collected) {
        let Collected {
            ready,
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
            transform_callbacks,
            ..
        } = collected;
        for (callback, unlock_on_success, result) in ready {
            if unlock_on_success && result.is_ok() {
                self.reactive
                    .borrow_mut()
                    .lifecycle
                    .session_unlock_requested = true;
            }
            let args = match result {
                Ok(()) => vec![IpcValue::Boolean(true), IpcValue::Nil],
                Err(error) => vec![
                    IpcValue::Boolean(false),
                    IpcValue::String(error.to_string()),
                ],
            };
            if let Err(message) =
                self.run_handler(|ctx, limits| execute_handler_args(ctx, &callback, &args, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("PAM callback: {message}"));
            }
        }
        for (callback, outcome, url, json, handle) in http_answers {
            let _span = crate::profile::span(|| "http callback".to_owned());
            // Cancelled by an earlier callback in this same batch.
            if handle.cancelled.get() {
                continue;
            }
            handle.done.set(true);
            if let Err(message) = self.run_handler(|ctx, limits| {
                crate::api_http::execute_http_handler(ctx, &callback, outcome, &url, &json, limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("http callback: {message}"));
            }
        }
        for call in &io_calls {
            let _span = crate::profile::span(|| "I/O callback".to_owned());
            if let Err(message) =
                self.run_handler(|ctx, limits| crate::api_io::execute_io_call(ctx, call, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("I/O callback: {message}"));
            }
        }
        for call in &watch_calls {
            let _span = crate::profile::span(|| "fs.watch callback".to_owned());
            if let Err(message) = self
                .run_handler(|ctx, limits| crate::api_watch::execute_watch_call(ctx, call, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("fs.watch callback: {message}"));
            }
        }
        if io_more || watch_more {
            // One turn's share is spent; the rest is for the next turn,
            // which this makes come at once rather than at the next event.
            morf_io::wake_all();
        }
        for DueTimer {
            origin,
            id,
            node,
            repeat,
            handler: callback,
            ..
        } in timers
        {
            let _span = crate::profile::span(|| format!("timer {origin}"));
            // Collected before this turn's other callbacks, loader drops and
            // earlier timers ran, any of which may have stopped this one or
            // torn its node down. A timer fires only if it is still wanted.
            if !timer_still_due(&mut self.reactive.borrow_mut(), id, node, repeat) {
                continue;
            }
            if let Err(message) =
                self.run_handler(|ctx, limits| execute_handler_args(ctx, &callback, &[], limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("timer callback: {message}"));
            }
        }
        for (callback, revision) in transform_callbacks {
            let _span = crate::profile::span(|| "transform callback".to_owned());
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_handler_args(
                    ctx,
                    &callback,
                    &[IpcValue::Integer(revision as i64)],
                    limits,
                )
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("transform callback: {message}"));
            }
        }
        for (callback, event) in pam_messages {
            let _span = crate::profile::span(|| "PAM session".to_owned());
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_pam_session_handler(ctx, &callback, event, limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("PAM session: {message}"));
            }
        }
        for (callback, event) in greetd_messages {
            if let Err(message) = self
                .run_handler(|ctx, limits| execute_greetd_handler(ctx, &callback, event, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("greetd: {message}"));
            }
        }
        for (callback, call) in dbus_calls {
            let _span = crate::profile::span(|| "D-Bus call handler".to_owned());
            if let Err(message) = self
                .run_handler(|ctx, limits| execute_dbus_call_handler(ctx, &callback, call, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("D-Bus call: {message}"));
            }
        }
        for (callback, reply) in dbus_replies {
            let _span = crate::profile::span(|| "D-Bus reply callback".to_owned());
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_dbus_reply_handler(ctx, &callback, reply, limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("D-Bus reply callback: {message}"));
            }
        }
        for (id, callback, kind, event) in dbus_signals {
            let _span = crate::profile::span(|| "D-Bus signal callback".to_owned());
            // Closed by an earlier callback in this same batch: what was
            // already read for it is not delivered, because "after close,
            // nothing" is the promise `close` makes.
            if !self
                .reactive
                .borrow()
                .dbus_signals
                .iter()
                .any(|subscription| subscription.id == id)
            {
                continue;
            }
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_dbus_signal_handler(ctx, &callback, event, kind, limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("D-Bus signal: {message}"));
            }
        }
        for (callback, event) in udev_events {
            let _span = crate::profile::span(|| "udev callback".to_owned());
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_dbus_handler(ctx, &callback, udev_event_value(event), limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("udev callback: {message}"));
            }
        }
        for (callback, items) in status_updates {
            let _span = crate::profile::span(|| "status notifier callback".to_owned());
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_dbus_handler(ctx, &callback, status_notifier_value(items), limits)
            }) {
                self.reactive.borrow_mut().log(
                    LogLevel::Warn,
                    format!("status notifier callback: {message}"),
                );
            }
        }
    }
}
