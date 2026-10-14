//! Sources polled every turn for what came in: udev monitors, tray hosts
//! and HTTP requests in flight.

use std::rc::Rc;
use std::time::{Duration, Instant};

use morf_runtime::Handler;
use morf_system::{StatusNotifierAddress, StatusNotifierHost, UdevEvent, UdevMonitor};

use crate::api::io::http::{HttpHandleState, JsonKinds, PendingHttp};
use crate::runtime::pending::{PendingStatusNotifier, PendingUdev};
use crate::runtime::services::HttpAnswer;

/// The most tray hosts one runtime keeps.
const MAX_STATUS_NOTIFIERS: usize = 4;

#[derive(Default)]
pub(crate) struct Feeds {
    udev: Vec<PendingUdev>,
    trays: Vec<PendingStatusNotifier>,
    http: Vec<PendingHttp>,
}

impl Feeds {
    /// Hears `monitor`'s devices for `callback`.
    pub(crate) fn watch_devices(&mut self, monitor: UdevMonitor, callback: Handler) {
        self.udev.push(PendingUdev { monitor, callback });
    }

    /// The device events of this turn, and the monitors' errors.
    pub(crate) fn take_device_events(&mut self) -> (Vec<(Handler, UdevEvent)>, Vec<String>) {
        let (mut events, mut errors) = (Vec::new(), Vec::new());
        for subscription in &mut self.udev {
            let mut drained = false;
            for _ in 0..32 {
                match subscription.monitor.next_event(Duration::ZERO) {
                    Ok(Some(event)) => events.push((subscription.callback.clone(), event)),
                    Ok(None) => {
                        drained = true;
                        break;
                    }
                    Err(error) => {
                        errors.push(error.to_string());
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
        (events, errors)
    }

    /// Whether another tray host would pass the limit.
    pub(crate) fn trays_full(&self) -> bool {
        self.trays.len() >= MAX_STATUS_NOTIFIERS
    }

    /// Hears `host`'s tray items for `callback`.
    pub(crate) fn watch_tray(&mut self, host: StatusNotifierHost, callback: Handler) {
        self.trays.push(PendingStatusNotifier { host, callback });
    }

    /// The tray lists that changed this turn, and the hosts' errors.
    pub(crate) fn take_tray_updates(
        &mut self,
    ) -> (Vec<(Handler, Vec<StatusNotifierAddress>)>, Vec<String>) {
        let (mut updates, mut errors) = (Vec::new(), Vec::new());
        for subscription in &mut self.trays {
            match subscription.host.poll_changed() {
                Ok(Some(items)) => updates.push((subscription.callback.clone(), items)),
                Ok(None) => {}
                Err(error) => errors.push(error.to_string()),
            }
        }
        (updates, errors)
    }

    /// The soonest a tray host retries.
    pub(crate) fn next_tray_deadline(&self) -> Option<Instant> {
        self.trays
            .iter()
            .filter_map(|subscription| subscription.host.next_deadline())
            .min()
    }

    /// Lets go of the requests cancelled meanwhile. An error when there is
    /// still no room for another.
    pub(crate) fn make_room(&mut self) -> Result<(), morf_io::Error> {
        self.http.retain(|entry| !entry.handle.cancelled.get());
        morf_io::check_pending(self.http.len())
    }

    /// Starts `request`; `callback` is owed its answer.
    pub(crate) fn request(
        &mut self,
        request: morf_io::HttpRequest,
        callback: Option<Handler>,
        json: JsonKinds,
    ) -> Rc<HttpHandleState> {
        let handle = Rc::new(HttpHandleState::default());
        self.http.push(PendingHttp {
            url: request.url.clone(),
            task: morf_io::HttpTask::start(request),
            callback,
            handle: Rc::clone(&handle),
            json,
        });
        handle
    }

    /// The answers that came. A cancelled request leaves without a word,
    /// and dropping its task stops the worker; a finished one leaves with
    /// its answer.
    pub(crate) fn take_http_answers(&mut self) -> Vec<HttpAnswer> {
        let mut answers = Vec::new();
        self.http.retain_mut(|entry| {
            if entry.handle.cancelled.get() {
                return false;
            }
            let Some(outcome) = entry.task.poll() else {
                return true;
            };
            if let Some(callback) = entry.callback.take() {
                answers.push((
                    callback,
                    outcome,
                    std::mem::take(&mut entry.url),
                    entry.json.clone(),
                    Rc::clone(&entry.handle),
                ));
            } else {
                entry.handle.done.set(true);
            }
            false
        });
        answers
    }
}
