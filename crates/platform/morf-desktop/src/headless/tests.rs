use std::sync::Arc;

use super::*;

fn workspace(key: &str, output: &str, active: bool) -> WorkspaceInfo {
    WorkspaceInfo {
        key: key.to_owned(),
        name: key.to_owned(),
        output: output.to_owned(),
        active,
        activatable: true,
        removable: true,
        assignable: true,
        ..WorkspaceInfo::default()
    }
}

fn window(identifier: &str) -> ToplevelInfo {
    ToplevelInfo {
        identifier: identifier.to_owned(),
        controllable: true,
        ..ToplevelInfo::default()
    }
}

#[test]
fn reports_nothing_and_refuses_everything_by_default() {
    let mut desktop = HeadlessDesktop::new();
    assert_eq!(desktop.capabilities(), DesktopCapabilities::default());
    assert!(desktop.workspaces().is_empty() && desktop.toplevels().is_empty());
    assert!(!desktop.take_workspaces_changed() && !desktop.take_toplevels_changed());
    assert!(!desktop.activate_workspace("1"));
    assert!(!desktop.set_selection(vec![("text/plain".into(), Arc::new(b"x".to_vec()))], false));
    assert!(!desktop.capture_output_image(1, false, None));
    assert!(desktop.set_gamma(None, GammaSettings::default()).is_err());
    assert!(!desktop.set_output_power(OutputPowerMode::Off));
    assert!(!desktop.set_idle_timeouts(&[(1000, false)]));
    assert!(desktop.next_event().is_none());
    assert_eq!(desktop.requests().len(), 6);
    assert_eq!(desktop.idle_timeouts(), &[(1000, false)]);
}

#[test]
fn records_requests_in_order() {
    let mut desktop = HeadlessDesktop::new();
    desktop.activate_workspace("2");
    desktop.control_toplevel("a", ToplevelAction::Close);
    assert_eq!(
        desktop.take_requests(),
        vec![
            DesktopRequest::ActivateWorkspace("2".into()),
            DesktopRequest::ControlToplevel {
                identifier: "a".into(),
                action: ToplevelAction::Close,
            },
        ]
    );
    assert!(desktop.requests().is_empty());
}

#[test]
fn activating_a_workspace_moves_the_active_one_on_its_output() {
    let mut desktop = HeadlessDesktop::new()
        .with_capabilities(DesktopCapabilities::ALL)
        .with_outputs(["A", "B"])
        .with_workspaces(vec![
            workspace("2", "A", false),
            workspace("1", "A", true),
            workspace("3", "B", true),
        ]);
    assert!(desktop.take_workspaces_changed());
    assert!(desktop.activate_workspace("2"));
    assert!(desktop.take_workspaces_changed());
    let listed = desktop.workspaces();
    let active: Vec<_> = listed
        .iter()
        .filter(|w| w.active)
        .map(|w| w.key.as_str())
        .collect();
    assert_eq!(active, ["2", "3"]);
    assert!(!desktop.activate_workspace("missing"));
    assert!(desktop.assign_workspace("1", "B"));
    assert!(!desktop.assign_workspace("1", "C"));
    assert!(desktop.remove_workspace("3"));
    assert_eq!(desktop.workspaces().len(), 2);
}

#[test]
fn controls_only_controllable_windows() {
    let mut fixed = window("fixed");
    fixed.controllable = false;
    let mut desktop = HeadlessDesktop::new()
        .with_capabilities(DesktopCapabilities::ALL)
        .with_toplevels(vec![window("a"), window("b"), fixed]);
    desktop.take_toplevels_changed();
    assert!(desktop.control_toplevel("b", ToplevelAction::Activate));
    assert!(desktop.take_toplevels_changed());
    assert!(desktop.toplevels()[1].activated && !desktop.toplevels()[0].activated);
    assert!(!desktop.control_toplevel("fixed", ToplevelAction::Close));
    assert!(desktop.control_toplevel("a", ToplevelAction::Close));
    assert_eq!(desktop.toplevels().len(), 2);
}

#[test]
fn the_selection_set_here_can_be_read_back() {
    let mut desktop = HeadlessDesktop::new().with_capabilities(DesktopCapabilities::ALL);
    let data = vec![(
        "text/plain;charset=utf-8".to_owned(),
        Arc::new(b"hi".to_vec()),
    )];
    assert!(desktop.set_selection(data, false));
    let Some(DesktopEvent::Selection {
        primary: false,
        offer: Some(offer),
    }) = desktop.next_event()
    else {
        panic!("no selection announced");
    };
    assert!(desktop.owns_offer(offer.id));
    desktop.read_offer(7, offer.id, "text");
    assert_eq!(
        desktop.next_event(),
        Some(DesktopEvent::OfferRead {
            request_id: 7,
            result: Ok(b"hi".to_vec()),
        })
    );
    desktop.read_offer(8, offer.id, "image/png");
    assert!(matches!(
        desktop.next_event(),
        Some(DesktopEvent::OfferRead {
            request_id: 8,
            result: Err(_)
        })
    ));
    assert!(desktop.clear_selection(false));
    assert!(matches!(
        desktop.next_event(),
        Some(DesktopEvent::Selection { offer: None, .. })
    ));
}

#[test]
fn a_capture_answers_with_a_blank_frame_for_known_targets() {
    let mut desktop = HeadlessDesktop::new()
        .with_capabilities(DesktopCapabilities::ALL)
        .with_outputs(["A"])
        .with_toplevels(vec![window("a")]);
    assert!(desktop.capture_output_image(1, true, Some("A")));
    assert!(!desktop.capture_output(2, false, Some("B")));
    assert!(desktop.capture_window(3, "a", false));
    assert!(!desktop.capture_window(4, "z", false));
    let answered: Vec<u64> = std::iter::from_fn(|| desktop.next_event())
        .map(|event| match event {
            DesktopEvent::Screencopy {
                request_id,
                result: Ok(frame),
            } => {
                assert_eq!((frame.width, frame.height, frame.pixels.len()), (1, 1, 4));
                request_id
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(answered, [1, 3]);
}

#[test]
fn pushed_news_comes_out_in_order() {
    let mut desktop = HeadlessDesktop::new();
    let idle = DesktopEvent::Idle {
        timeout_ms: 500,
        input_only: false,
        idle: true,
    };
    desktop.push_event(idle.clone());
    assert!(desktop.dispatch_pending().is_ok());
    assert_eq!(desktop.next_event(), Some(idle));
    assert_eq!(desktop.next_event(), None);
}
