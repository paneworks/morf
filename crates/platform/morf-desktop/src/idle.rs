//! `ext-idle-notify`: being told when the person has stopped, and started
//! again, after each threshold a configuration asked about.

use wayland_client::globals::GlobalList;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::ExtIdleNotifierV1,
};

use crate::{Desktop, DesktopEvent, DesktopState};

/// A threshold: milliseconds, and whether it counts input only (ignoring
/// whatever inhibits idleness).
type IdleKey = (u32, bool);

#[derive(Default)]
pub(crate) struct IdleState {
    notifier: Option<ExtIdleNotifierV1>,
    notifications: Vec<ExtIdleNotificationV1>,
    timeouts: Vec<IdleKey>,
}

impl IdleState {
    pub(crate) fn bind(globals: &GlobalList, qh: &QueueHandle<DesktopState>) -> Self {
        Self {
            notifier: globals.bind(qh, 1..=2, ()).ok(),
            ..Self::default()
        }
    }

    /// Every notification made again, on `seat`: a seat arrived, or the
    /// thresholds were replaced outright.
    pub(crate) fn refresh(&mut self, seat: Option<&WlSeat>, qh: &QueueHandle<DesktopState>) {
        for notification in self.notifications.drain(..) {
            notification.destroy();
        }
        let (Some(notifier), Some(seat)) = (&self.notifier, seat) else {
            return;
        };
        self.notifications = self
            .timeouts
            .iter()
            .map(|&key| notification(notifier, seat, key, qh))
            .collect();
    }

    /// Only what changed: a threshold still wanted keeps its notification
    /// (and its running clock), one no longer wanted goes, a new one comes.
    fn reconcile(&mut self, seat: Option<&WlSeat>, qh: &QueueHandle<DesktopState>) {
        let existing = self
            .notifications
            .iter()
            .filter_map(|notification| notification.data::<IdleKey>().copied())
            .collect::<Vec<_>>();
        let (removed, added) = idle_changes(&existing, &self.timeouts);
        self.notifications.retain(|notification| {
            let keep = notification
                .data::<IdleKey>()
                .is_some_and(|key| !removed.contains(key));
            if !keep {
                notification.destroy();
            }
            keep
        });
        let (Some(notifier), Some(seat)) = (&self.notifier, seat) else {
            return;
        };
        for key in added {
            self.notifications.push(notification(notifier, seat, key, qh));
        }
    }

    fn set(&mut self, timeouts: &[IdleKey]) {
        self.timeouts = timeouts.iter().copied().take(64).collect();
        self.timeouts.sort_unstable();
        self.timeouts.dedup();
    }
}

fn notification(
    notifier: &ExtIdleNotifierV1,
    seat: &WlSeat,
    (timeout, input_only): IdleKey,
    qh: &QueueHandle<DesktopState>,
) -> ExtIdleNotificationV1 {
    // Version 2 of the protocol added the input variant, which counts only
    // the person and ignores inhibitors. An older compositor gets the
    // ordinary one for the same key, so the caller's callback still fires --
    // on inhibited idleness rather than never, which is the better of the two
    // ways to degrade.
    if input_only && notifier.version() >= 2 {
        notifier.get_input_idle_notification(timeout, seat, qh, (timeout, input_only))
    } else {
        notifier.get_idle_notification(timeout, seat, qh, (timeout, input_only))
    }
}

/// The thresholds to let go of and the ones to ask for, going from `held`
/// to `wanted`.
fn idle_changes(held: &[IdleKey], wanted: &[IdleKey]) -> (Vec<IdleKey>, Vec<IdleKey>) {
    let removed = held
        .iter()
        .filter(|key| !wanted.contains(key))
        .copied()
        .collect();
    let mut added = wanted
        .iter()
        .filter(|key| !held.contains(key))
        .copied()
        .collect::<Vec<_>>();
    added.sort_unstable();
    added.dedup();
    (removed, added)
}

impl Desktop {
    /// Replaces the idle thresholds; returns whether the compositor tells
    /// idleness at all.
    pub fn set_idle_timeouts(&mut self, timeouts: &[IdleKey]) -> bool {
        self.state.idle.set(timeouts);
        let seat = self.state.seat();
        let qh = self.handle();
        self.state.idle.refresh(seat.as_ref(), &qh);
        self.state.idle.notifier.is_some()
    }

    /// Changes the idle thresholds in place: a notification for a threshold
    /// still wanted is kept (its clock keeps running), one no longer wanted
    /// is destroyed, a new one is created. ext-idle-notify allows both at
    /// any time, so a subscription made after startup applies at once.
    pub fn update_idle_timeouts(&mut self, timeouts: &[IdleKey]) -> bool {
        self.state.idle.set(timeouts);
        let seat = self.state.seat();
        let qh = self.handle();
        self.state.idle.reconcile(seat.as_ref(), &qh);
        self.state.idle.notifier.is_some()
    }
}

impl Dispatch<ExtIdleNotifierV1, ()> for DesktopState {
    fn event(
        _: &mut Self,
        _: &ExtIdleNotifierV1,
        _: <ExtIdleNotifierV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtIdleNotificationV1, IdleKey> for DesktopState {
    fn event(
        state: &mut Self,
        _: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        (timeout_ms, input_only): &IdleKey,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let idle = match event {
            ext_idle_notification_v1::Event::Idled => true,
            ext_idle_notification_v1::Event::Resumed => false,
            _ => return,
        };
        state.events.push_back(DesktopEvent::Idle {
            timeout_ms: *timeout_ms,
            input_only: *input_only,
            idle,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::idle_changes;

    #[test]
    fn idle_reconciliation_touches_only_what_changed() {
        let held = [(60_000, false), (300_000, true)];
        let (removed, added) = idle_changes(&held, &[(5_000, false), (60_000, false)]);
        assert_eq!(removed, [(300_000, true)]);
        assert_eq!(added, [(5_000, false)], "the minute keeps its running clock");
        let (removed, added) = idle_changes(&held, &held);
        assert!(removed.is_empty() && added.is_empty());
    }
}
