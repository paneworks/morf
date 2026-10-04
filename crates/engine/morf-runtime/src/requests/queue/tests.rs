use std::any::Any;
use std::rc::Rc;

use super::*;
use crate::handler::{HandlerId, HandlerRegistry};

struct Nowhere;

impl HandlerRegistry for Nowhere {
    fn release(&self, _id: HandlerId) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn handler(id: u64) -> Handler {
    Handler::new(HandlerId(id), Rc::new(Nowhere))
}

#[test]
fn only_the_last_gamma_for_an_output_waits() {
    let mut requests = Requests::default();
    for temperature in [3000.0, 4000.0, 5000.0] {
        requests
            .queue_gamma(GammaRequest {
                output: Some("DP-1".into()),
                set: Some((temperature, 1.0, 1.0)),
            })
            .unwrap();
    }
    requests
        .queue_gamma(GammaRequest {
            output: None,
            set: None,
        })
        .unwrap();
    let taken = requests.take_gamma_requests();
    assert_eq!(taken.len(), 2);
    assert_eq!(taken[0].set, Some((5000.0, 1.0, 1.0)));
    assert!(requests.take_gamma_requests().is_empty());
}

#[test]
fn a_full_queue_refuses_with_the_limit_named() {
    let mut requests = Requests::default();
    for _ in 0..64 {
        requests.queue_output_power(true).unwrap();
    }
    assert_eq!(
        requests.queue_output_power(false),
        Err("output power request limit reached".to_owned())
    );
    for _ in 0..4 {
        requests.queue_drag(DragRequest::default(), None).unwrap();
    }
    assert_eq!(
        requests.queue_drag(DragRequest::default(), Some(handler(1))),
        Err("drag request limit reached".to_owned())
    );
    // A refused drag keeps no callback to call.
    assert!(requests.drag_end_callbacks.is_empty());
    for _ in 0..256 {
        requests
            .queue_text_input(TextInputRequest::Disable)
            .unwrap();
    }
    assert!(
        requests
            .queue_text_input(TextInputRequest::Disable)
            .is_err()
    );
    assert_eq!(requests.take_text_input_requests().len(), 256);
}

#[test]
fn subscribing_asks_for_the_role_once_taken() {
    let mut requests = Requests::default();
    requests.subscribe_input_method(handler(2)).unwrap();
    assert!(requests.take_input_method_enable_request());
    assert!(!requests.take_input_method_enable_request());
    requests.subscribe_text_input(handler(3)).unwrap();
    assert!(requests.take_text_input_enable_request());
    assert_eq!(requests.text_input_callbacks.len(), 1);
}

#[test]
fn inhibit_changes_are_heard_once_each_time_they_are_asked() {
    let mut requests = Requests::default();
    assert_eq!(requests.take_idle_inhibit_change(), None);
    requests.set_idle_inhibited(true);
    assert_eq!(requests.take_idle_inhibit_change(), Some(true));
    assert_eq!(requests.take_idle_inhibit_change(), None);
    // Asked again with the same value: the host hears it again.
    requests.set_idle_inhibited(true);
    assert_eq!(requests.take_idle_inhibit_change(), Some(true));
    requests.set_shortcuts_inhibited(true);
    assert_eq!(requests.take_shortcuts_inhibit_change(), Some(true));
    assert_eq!(requests.take_shortcuts_inhibit_change(), None);
}

#[test]
fn idle_thresholds_are_listed_in_order_and_their_change_taken_once() {
    let mut requests = Requests::default();
    requests
        .idle_callbacks
        .insert((300_000, false), vec![(0, handler(4))]);
    requests
        .idle_callbacks
        .insert((60_000, true), vec![(1, handler(5))]);
    requests.idle_timeouts_changed = true;
    assert_eq!(
        requests.take_idle_timeouts_change(),
        Some(vec![(60_000, true), (300_000, false)])
    );
    assert_eq!(requests.take_idle_timeouts_change(), None);
    assert_eq!(requests.idle_subscribers(60_000, true).len(), 1);
    assert!(requests.idle_subscribers(60_000, false).is_empty());
}
