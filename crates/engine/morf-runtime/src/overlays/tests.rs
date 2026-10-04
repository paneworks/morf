//! The overlay stack with no VM: Escape and a press outside close the top
//! one, placement moves it only when it moved, focus goes back, and
//! `on_close` hears why.

use std::any::Any;
use std::rc::Rc;

use morf_scene::Element;

use super::*;
use crate::{HandlerId, HandlerRegistry};

struct Nowhere;

impl HandlerRegistry for Nowhere {
    fn release(&self, _: HandlerId) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn overlay(root: NodeHandle, content: NodeHandle) -> Overlay {
    Overlay {
        root,
        wrapper: content,
        content,
        anchor: None,
        placement: Placement::parse("center").unwrap(),
        gap: 4.0,
        margin: 8.0,
        modal: false,
        escape: true,
        outside: true,
        on_close: None,
        restore: None,
        give_back: true,
        placed: None,
        tracked: false,
        except: Vec::new(),
    }
}

fn tree() -> (Scene, NodeHandle, NodeHandle, NodeHandle) {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let menu = scene.create(Element::Item);
    let item = scene.create(Element::Item);
    scene.reparent(menu, Some(root)).unwrap();
    scene.reparent(item, Some(menu)).unwrap();
    (scene, root, menu, item)
}

#[test]
fn escape_closes_the_top_one_that_lets_it() {
    let (_, root, menu, item) = tree();
    let mut overlays = Overlays::<()>::default();
    overlays.stack.push(overlay(root, menu));
    let mut dialog = overlay(root, item);
    dialog.escape = false;
    dialog.modal = true;
    overlays.stack.push(dialog);
    assert_eq!(overlays.focus_root(root), item);
    assert!(overlays.escape(root).is_none());
    overlays.stack.pop();
    assert_eq!(overlays.focus_root(root), root);
    assert_eq!(overlays.escape(root).map(|o| o.content), Some(menu));
    assert!(overlays.stack.is_empty());
}

#[test]
fn a_press_inside_or_on_the_anchor_is_not_outside() {
    let (mut scene, root, menu, item) = tree();
    let button = scene.create(Element::Item);
    scene.reparent(button, Some(root)).unwrap();
    let mut overlays = Overlays::<()>::default();
    let mut open = overlay(root, menu);
    open.anchor = Some(button);
    overlays.stack.push(open);
    assert!(overlays.press(&scene, root, Some(item), &[]).is_none());
    assert!(overlays.press(&scene, root, None, &[button]).is_none());
    assert_eq!(overlays.nodes(root), vec![menu, button]);
    assert!(overlays.press(&scene, root, Some(root), &[]).is_some());
    assert!(overlays.roots().is_empty());
}

#[test]
fn closes_queue_and_reopens_wait_for_them() {
    let (_, root, menu, item) = tree();
    let mut overlays = Overlays::<u8>::default();
    overlays.stack.push(overlay(root, menu));
    overlays.open_again(menu, || 1);
    assert!(overlays.reopening.is_empty() && overlays.is_open(menu));
    overlays.request_close(menu, "closed");
    overlays.open_again(menu, || 2);
    assert!(!overlays.is_open(menu) && overlays.reopening == [(menu, 2)]);
    overlays.stack.push(overlay(root, item));
    let closing = overlays.take_closing(|node| node != item);
    assert_eq!(closing, [(menu, "closed"), (item, "gone")]);
    assert!(overlays.remove(menu).is_some() && overlays.remove(menu).is_none());
}

#[test]
fn placement_moves_it_only_when_it_moved() {
    let (_, root, menu, _) = tree();
    let mut open = overlay(root, menu);
    assert_eq!(
        open.place((200.0, 100.0), (50.0, 20.0), None),
        Some((75.0, 40.0))
    );
    assert_eq!(open.place((200.0, 100.0), (50.0, 20.0), None), None);
}

#[test]
fn focus_goes_back_when_it_was_inside() {
    let (scene, root, menu, item) = tree();
    let mut open = overlay(root, menu);
    open.restore = Some((root, true));
    assert_eq!(
        open.give_focus_back(&scene, Some(item), |_| true),
        Some((Some(root), FocusReason::Keyboard))
    );
    assert_eq!(open.give_focus_back(&scene, Some(root), |_| true), None);
    assert_eq!(
        open.give_focus_back(&scene, None, |_| false),
        Some((None, FocusReason::Keyboard))
    );
    open.give_back = false;
    assert_eq!(open.give_focus_back(&scene, None, |_| true), None);
}

#[test]
fn on_close_hears_the_reason() {
    struct Recorder(Vec<Vec<IpcValue>>);
    impl Handlers for Recorder {
        fn call(&mut self, _: &Handler, args: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
            self.0.push(args.to_vec());
            Ok(Vec::new())
        }
    }
    let (_, root, menu, _) = tree();
    let mut open = overlay(root, menu);
    let mut recorder = Recorder(Vec::new());
    open.notify_closed(&mut recorder, "escape").unwrap();
    assert!(recorder.0.is_empty());
    open.on_close = Some(Handler::new(HandlerId(1), Rc::new(Nowhere)));
    open.notify_closed(&mut recorder, "escape").unwrap();
    assert_eq!(recorder.0, [vec![IpcValue::String("escape".into())]]);
}
