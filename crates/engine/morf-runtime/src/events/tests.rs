//! Events with no VM: key targets and routes, button and wheel answers,
//! hover and press, a Flickable's scroll, delivery and keys bubbling up.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use morf_scene::{Element, NodeHandle, Scene};
use morf_value::IpcValue;

use super::routing::*;
use super::*;
use crate::{Handler, HandlerId, HandlerRegistry};

struct Nowhere;

impl HandlerRegistry for Nowhere {
    fn release(&self, _: HandlerId) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn handler(id: u64) -> Handler {
    Handler::new(HandlerId(id), Rc::new(Nowhere))
}

/// A root with a pointer area, a text input and a key-handling item under
/// it, the pointer area holding a child of its own.
fn tree() -> (Scene, [NodeHandle; 5]) {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let area = scene.create(Element::MouseArea);
    let child = scene.create(Element::Item);
    let input = scene.create(Element::TextInput);
    let keys = scene.create(Element::Item);
    scene.reparent(area, Some(root)).unwrap();
    scene.reparent(child, Some(area)).unwrap();
    scene.reparent(input, Some(root)).unwrap();
    scene.reparent(keys, Some(root)).unwrap();
    (scene, [root, area, child, input, keys])
}

#[test]
fn keys_go_to_text_inputs_and_key_handlers_in_tree_order() {
    let (scene, [root, area, child, input, keys]) = tree();
    let mut events = Events::default();
    events.set(keys, UiEvent::KeyPressed, Some(handler(1)));
    assert!(takes_keys(&scene, &events, input));
    assert!(takes_keys(&scene, &events, keys));
    assert!(!takes_keys(&scene, &events, area));
    assert_eq!(key_targets_in(&scene, &events, root), vec![input, keys]);
    assert_eq!(
        next_key_target(&scene, &events, root, Some(keys)),
        Some(input)
    );
    assert_eq!(next_key_target(&scene, &events, root, None), Some(input));
    // A key pressed with the area's child focused goes up to what takes keys.
    assert_eq!(key_route(&scene, &events, child), None);
    events.set(area, UiEvent::KeyReleased, Some(handler(2)));
    assert_eq!(key_route(&scene, &events, child), Some(area));
    assert!(node_in_subtree(&scene, root, child));
    assert!(!node_in_subtree(&scene, area, input));
}

#[test]
fn return_and_space_click_a_pointer_area_without_keys_of_its_own() {
    let (scene, [_, area, ..]) = tree();
    let mut events = Events::default();
    let plain = KeyModifiers::default();
    assert!(!activates_by_key(&scene, &events, area, 0xff0d, plain));
    events.set(area, UiEvent::Clicked, Some(handler(1)));
    assert!(activates_by_key(&scene, &events, area, 0xff0d, plain));
    assert!(activates_by_key(&scene, &events, area, 0x20, plain));
    let ctrl = KeyModifiers {
        ctrl: true,
        ..plain
    };
    assert!(!activates_by_key(&scene, &events, area, 0x20, ctrl));
    assert!(!activates_by_key(&scene, &events, area, 0x61, plain));
}

#[test]
fn buttons_and_the_wheel_go_where_they_are_taken() {
    let (mut scene, [_, area, child, input, _]) = tree();
    let mut events = Events::default();
    assert!(accepts_pointer_button(&scene, input, 0x110));
    assert!(!accepts_pointer_button(&scene, input, 0x111));
    scene.assign(area, "accepted_buttons", "right").unwrap();
    assert!(accepts_pointer_button(&scene, area, 0x111));
    assert!(!accepts_pointer_button(&scene, area, 0x110));
    assert!(!takes_wheel(&scene, &events, child));
    events.set(child, UiEvent::Wheel, Some(handler(1)));
    assert!(takes_wheel(&scene, &events, child));
}

#[test]
fn a_pointer_area_keeps_hovered_and_pressed() {
    let (mut scene, [root, area, ..]) = tree();
    assert_eq!(
        pointer_state_change(&scene, area, UiEvent::PointerEntered),
        Some(("hovered", true))
    );
    scene.assign(area, "hovered", true).unwrap();
    assert_eq!(
        pointer_state_change(&scene, area, UiEvent::PointerEntered),
        None
    );
    assert_eq!(
        pointer_state_change(&scene, area, UiEvent::Pressed),
        Some(("pressed", true))
    );
    // Only a pointer area keeps them.
    assert_eq!(pointer_state_change(&scene, root, UiEvent::Pressed), None);
}

#[test]
fn a_flickable_scrolls_inside_its_extent() {
    let mut scene = Scene::new();
    let flick = scene.create(Element::Flickable);
    scene.assign(flick, "content_y", 90.0).unwrap();
    assert_eq!(
        flickable_scroll(&scene, flick, (0.0, 30.0), (0.0, 100.0)),
        vec![("content_y", 100.0)]
    );
    assert!(flickable_scroll(&scene, flick, (0.0, 0.0), (0.0, 100.0)).is_empty());
    assert_eq!(
        flickable_scroll(&scene, flick, (-5.0, 0.0), (50.0, 100.0)),
        Vec::<(&str, f64)>::new()
    );
}

#[test]
fn removed_nodes_lose_their_handlers_and_watches() {
    let (_, [_, area, ..]) = tree();
    let mut events = Events::default();
    events.set(area, UiEvent::Clicked, Some(handler(1)));
    events.pointer_watch.read(area);
    events.forget(&[area].into_iter().collect());
    assert!(events.is_empty());
    assert!(events.pointer_watch.is_empty());
}

/// Records what delivery asked of it; key handlers answer from `replies`.
struct Host {
    scene: Scene,
    events: Events,
    log: Rc<RefCell<Vec<String>>>,
    replies: Vec<(u64, Vec<IpcValue>)>,
}

impl EventHost for Host {
    fn with_events<R>(&self, read: impl FnOnce(&Scene, &Events) -> R) -> R {
        read(&self.scene, &self.events)
    }
    fn assign_pointer_state(&mut self, node: NodeHandle, property: &str, value: bool) -> bool {
        self.log
            .borrow_mut()
            .push(format!("assign {property}={value}"));
        self.scene.assign(node, property, value).is_ok()
    }
    fn flush_after_event(&mut self) {
        self.log.borrow_mut().push("flush".to_owned());
    }
    fn run_event_handler(&mut self, handler: &Handler, args: &[IpcValue]) -> Result<(), String> {
        self.log
            .borrow_mut()
            .push(format!("handler {} with {}", handler.id().0, args.len()));
        Ok(())
    }
    fn run_key_handler(
        &mut self,
        handler: &Handler,
        _: &[IpcValue],
    ) -> Result<Vec<IpcValue>, String> {
        self.log
            .borrow_mut()
            .push(format!("key {}", handler.id().0));
        Ok(self
            .replies
            .iter()
            .find(|(id, _)| *id == handler.id().0)
            .map(|(_, values)| values.clone())
            .unwrap_or_default())
    }
    fn press_key(
        &mut self,
        node: NodeHandle,
        _: u32,
        _: Option<&str>,
        _: KeyModifiers,
        _: bool,
    ) -> bool {
        self.log.borrow_mut().push(format!("press {node:?}"));
        true
    }
    fn warn(&mut self, message: String) {
        self.log.borrow_mut().push(format!("warn {message}"));
    }
}

fn host() -> (Host, [NodeHandle; 5]) {
    let (scene, nodes) = tree();
    let host = Host {
        scene,
        events: Events::default(),
        log: Rc::default(),
        replies: Vec::new(),
    };
    (host, nodes)
}

#[test]
fn delivery_keeps_hover_then_runs_the_handler() {
    let (mut host, [_, area, child, ..]) = host();
    // No handler: the hover still lands, and its bindings are flushed.
    assert!(deliver(&mut host, area, UiEvent::PointerEntered, &[]));
    assert_eq!(*host.log.borrow(), ["assign hovered=true", "flush"]);
    host.log.borrow_mut().clear();
    // Nothing to keep and nothing to run.
    assert!(!deliver(&mut host, child, UiEvent::Clicked, &[]));
    host.events.set(area, UiEvent::Pressed, Some(handler(7)));
    assert!(deliver(&mut host, area, UiEvent::Pressed, &[IpcValue::Nil]));
    assert_eq!(
        *host.log.borrow(),
        ["assign pressed=true", "handler 7 with 1"]
    );
}

#[test]
fn a_key_a_handler_declines_goes_up_to_the_next_that_takes_keys() {
    let (mut host, [root, area, child, ..]) = host();
    host.scene.reparent(child, Some(area)).unwrap();
    host.events
        .set(child, UiEvent::KeyPressed, Some(handler(1)));
    host.events.set(root, UiEvent::KeyPressed, Some(handler(3)));
    host.replies.push((1, vec![IpcValue::Boolean(false)]));
    assert!(press_key_bubbling(
        &mut host,
        child,
        0x61,
        None,
        KeyModifiers::default(),
        false
    ));
    assert_eq!(*host.log.borrow(), ["key 1", "key 3"]);
}

#[test]
fn a_text_input_has_the_last_word_on_its_keys() {
    let (mut host, [_, _, _, input, _]) = host();
    assert!(press_key_bubbling(
        &mut host,
        input,
        0x61,
        Some("a"),
        KeyModifiers::default(),
        false
    ));
    assert_eq!(*host.log.borrow(), [format!("press {input:?}")]);
}
