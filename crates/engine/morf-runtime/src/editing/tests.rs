//! Editing with no VM: keys type, select, copy, paste and undo; a press
//! places the caret and a double press takes a word; one field has the
//! keyboard at a time; callbacks owed are queued.

use super::*;

const LEFT: u32 = 0xff51;
const RETURN: u32 = 0xff0d;
const BACKSPACE: u32 = 0xff08;

#[derive(Default)]
struct Host {
    scene: Scene,
    editing: Editing,
    clipboard: Option<String>,
    requests: Vec<TextInputRequest>,
    enabled: bool,
}

impl EditHost for Host {
    fn scene(&self) -> &Scene {
        &self.scene
    }
    fn editing(&self) -> &Editing {
        &self.editing
    }
    fn editing_mut(&mut self) -> &mut Editing {
        &mut self.editing
    }
    fn assign(&mut self, node: NodeHandle, property: &str, value: Value) -> Result<(), String> {
        self.scene
            .assign(node, property, value)
            .map_err(|error| error.to_string())
    }
    fn warn(&mut self, message: String) {
        panic!("{message}");
    }
    fn clipboard_text(&self) -> Option<String> {
        self.clipboard.clone()
    }
    fn copy(&mut self, text: String) {
        self.clipboard = Some(text);
    }
    fn text_input(&mut self, request: TextInputRequest) {
        self.requests.push(request);
    }
    fn enable_text_input(&mut self) {
        self.enabled = true;
    }
}

fn field(host: &mut Host, text: &str) -> NodeHandle {
    let node = host.scene.create(Element::TextInput);
    host.scene.assign(node, "text", text).unwrap();
    register(host, node);
    node
}

fn text(host: &Host, node: NodeHandle) -> &str {
    host.scene.string_value(node, "text").unwrap()
}

fn ctrl() -> KeyModifiers {
    KeyModifiers {
        ctrl: true,
        ..KeyModifiers::default()
    }
}

#[test]
fn keys_type_select_copy_paste_and_undo() {
    let mut host = Host::default();
    let node = field(&mut host, "hello");
    assert_eq!(host.scene.number(node, "cursor_position").ok(), Some(5.0));
    let none = KeyModifiers::default();
    assert_eq!(
        key(&mut host, node, 0x21, Some("!"), none),
        KeyOutcome::Handled
    );
    assert_eq!(text(&host, node), "hello!");
    key(&mut host, node, BACKSPACE, None, none);
    key(&mut host, node, LEFT, None, none);
    assert_eq!(host.scene.number(node, "cursor_position").ok(), Some(4.0));
    key(&mut host, node, u32::from('a'), None, ctrl());
    key(&mut host, node, u32::from('c'), None, ctrl());
    assert_eq!(host.clipboard.as_deref(), Some("hello"));
    key(&mut host, node, u32::from('v'), None, ctrl());
    key(&mut host, node, u32::from('v'), None, ctrl());
    assert_eq!(text(&host, node), "hellohello");
    key(&mut host, node, u32::from('z'), None, ctrl());
    assert_eq!(text(&host, node), "hello");
    let changed = host
        .editing
        .events
        .iter()
        .filter(|(_, event, _)| *event == UiEvent::TextChanged)
        .count();
    // The first paste replaced "hello" with "hello": no change to announce.
    assert_eq!(changed, 4);
    assert_eq!(
        key(&mut host, node, 0xffbe, None, none),
        KeyOutcome::Ignored
    );
}

#[test]
fn return_accepts_a_single_line() {
    let mut host = Host::default();
    let node = field(&mut host, "go");
    key(&mut host, node, RETURN, None, KeyModifiers::default());
    assert_eq!(text(&host, node), "go");
    assert_eq!(
        host.editing.events.last(),
        Some(&(node, UiEvent::Accepted, vec![IpcValue::String("go".into())]))
    );
}

#[test]
fn one_field_has_the_keyboard() {
    let mut host = Host::default();
    let first = field(&mut host, "a");
    let second = field(&mut host, "b");
    set_focus(&mut host, first, true);
    assert_eq!(host.editing.focused, Some(first));
    assert!(host.enabled);
    set_focus(&mut host, second, true);
    assert_eq!(host.editing.focused, Some(second));
    assert_eq!(host.scene.bool_value(first, "focus").ok(), Some(false));
    set_focus(&mut host, second, false);
    assert_eq!(host.editing.focused, None);
    assert!(matches!(
        host.requests.last(),
        Some(TextInputRequest::Disable)
    ));
    let focus_events = host
        .editing
        .events
        .iter()
        .filter(|(_, event, _)| *event == UiEvent::FocusChanged)
        .count();
    assert_eq!(focus_events, 4);
}

#[test]
fn a_double_press_takes_a_word() {
    let mut host = Host::default();
    let node = field(&mut host, "one two");
    host.scene.assign(node, "font_size", 10.0).unwrap();
    // A stand-in grid of 6px advances: x 27 is in "two".
    press(&mut host, node, (27.0, 2.0));
    release(&mut host, node);
    press(&mut host, node, (27.0, 2.0));
    assert_eq!(selected_text(&mut host, node), "two");
    assert!(drag(&mut host, node, (1.0, 2.0)));
    assert_eq!(selected_text(&mut host, node), "one two");
}

#[test]
fn a_configuration_write_is_taken_in_and_not_announced() {
    let mut host = Host::default();
    let node = field(&mut host, "abc");
    host.scene.assign(node, "text", "xyz!").unwrap();
    assert!(pull(&mut host, node));
    assert!(insert(&mut host, node, "?"));
    assert_eq!(text(&host, node), "xyz!?");
    assert_eq!(host.editing.events.len(), 1);
    assert!(history(&mut host, node, false));
    assert_eq!(text(&host, node), "xyz!");
    host.editing.forget(&HashSet::from([node]));
    assert!(host.editing.inputs.is_empty() && host.editing.events.is_empty());
    assert_eq!(tracked(&host.editing), []);
}

#[test]
fn draining_is_bounded() {
    let node = Scene::new().create(Element::Item);
    let mut editing = Editing::default();
    assert!(!editing.start_draining(false));
    editing.events.push((node, UiEvent::Escape, Vec::new()));
    assert!(!editing.start_draining(true));
    assert!(editing.start_draining(false));
    assert!(!editing.start_draining(false));
    assert!(editing.finish_draining() && !editing.draining);
    assert_eq!(
        key_args(0xff1b, None, ctrl(), Some(false))[2],
        IpcValue::String("ctrl".into())
    );
}
