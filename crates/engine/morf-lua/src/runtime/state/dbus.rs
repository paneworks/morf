//! What `morf.dbus` keeps between turns: the signal subscriptions, the calls
//! waiting on an answer, and the names and services this runtime serves.

use std::cell::RefCell;
use std::rc::Weak;
use std::time::{Duration, Instant};

use morf_io::{DbusCall, DbusHandlers, DbusService, DbusSignalEvent, DbusValue, SharedService};
use morf_runtime::Handler;

use crate::runtime::pending::{DbusSignalKind, PendingDbusReply, PendingDbusSignal};

#[derive(Default)]
pub(crate) struct DbusState {
    signals: Vec<PendingDbusSignal>,
    next_signal_id: u64,
    replies: Vec<PendingDbusReply>,
    owned_names: Vec<Weak<RefCell<DbusService>>>,
    services: DbusHandlers<Handler>,
}

impl DbusState {
    /// How many signal subscriptions are open.
    pub(crate) fn subscription_count(&self) -> usize {
        self.signals.len()
    }

    /// Opens a subscription and names it for its handle.
    pub(crate) fn subscribe(
        &mut self,
        signal: morf_io::DbusSignal,
        callback: Handler,
        kind: DbusSignalKind,
    ) -> u64 {
        self.next_signal_id += 1;
        let id = self.next_signal_id;
        self.signals.push(PendingDbusSignal {
            id,
            signal,
            callback,
            kind,
        });
        id
    }

    /// Closes subscription `id`. Returns whether it was open.
    pub(crate) fn unsubscribe(&mut self, id: u64) -> bool {
        let before = self.signals.len();
        self.signals.retain(|entry| entry.id != id);
        before != self.signals.len()
    }

    /// Whether subscription `id` is still open.
    pub(crate) fn is_subscribed(&self, id: u64) -> bool {
        self.signals.iter().any(|entry| entry.id == id)
    }

    /// Every signal heard since the last turn, with its subscription.
    pub(crate) fn take_signals(&self) -> Vec<(u64, Handler, DbusSignalKind, DbusSignalEvent)> {
        let mut heard = Vec::new();
        for subscription in &self.signals {
            while let Some(event) = subscription.signal.next_event(Duration::ZERO) {
                heard.push((
                    subscription.id,
                    subscription.callback.clone(),
                    subscription.kind,
                    event,
                ));
            }
        }
        heard
    }

    /// How many calls are waiting on an answer.
    pub(crate) fn reply_count(&self) -> usize {
        self.replies.len()
    }

    /// Waits on `reply` for `callback`.
    pub(crate) fn await_reply(&mut self, reply: morf_io::PendingReply, callback: Handler) {
        self.replies.push(PendingDbusReply { reply, callback });
    }

    /// The soonest a waiting call gives up.
    pub(crate) fn next_reply_deadline(&self) -> Option<Instant> {
        self.replies
            .iter()
            .filter_map(|entry| entry.reply.deadline())
            .min()
    }

    /// The answers that came, and the calls that missed their deadline; the
    /// rest wait for a later turn.
    pub(crate) fn take_replies(&mut self) -> Vec<(Handler, Result<DbusValue, morf_io::Error>)> {
        let mut answered = Vec::new();
        self.replies.retain(|entry| match entry.reply.try_take() {
            Some(reply) => {
                answered.push((entry.callback.clone(), reply));
                false
            }
            None => true,
        });
        answered
    }

    /// Remembers a name `morf.dbus.serve` took, to give back at the end.
    pub(crate) fn remember_name(&mut self, service: &SharedService) {
        morf_io::remember_name(&mut self.owned_names, service);
    }

    /// Every name this runtime still holds, to give back.
    pub(crate) fn take_names(&mut self) -> Vec<Weak<RefCell<DbusService>>> {
        std::mem::take(&mut self.owned_names)
    }

    /// Makes `callback` the one handler of `service`'s calls.
    pub(crate) fn serve(
        &mut self,
        service: &SharedService,
        callback: Handler,
    ) -> Result<(), morf_io::Error> {
        self.services.set(service, callback)
    }

    /// Stops answering `service`'s calls.
    pub(crate) fn stop_serving(&mut self, service: &SharedService) {
        self.services.remove(service);
    }

    /// The calls made to the services since the last turn.
    pub(crate) fn drain_calls(&self) -> Vec<(Handler, DbusCall)> {
        self.services.drain_calls()
    }
}
