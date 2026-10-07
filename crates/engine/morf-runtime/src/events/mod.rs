//! What happens to a node, as its handlers are told it: the events, the
//! property each handler is set as, and where a pointer was.

//!
//! And where each goes: the handler table ([`Events`]), the key targets and
//! where a key goes ([`routing`]), what each handler is called with
//! ([`args`]) and the delivery itself, keys bubbling up included
//! ([`deliver()`]).

pub mod args;
pub mod deliver;
mod held;
pub mod routing;
mod table;
#[cfg(test)]
mod tests;

pub use deliver::{EventHost, deliver, press_key_bubbling};
pub use held::{held, set_held};
pub use table::Events;

use morf_scene::NodeHandle;
use morf_value::IpcValue;

/// Event name accepted by Lua element handlers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiEvent {
    /// Pointer entered the target.
    PointerEntered,
    /// Pointer left the target.
    PointerExited,
    /// Pointer moved over or while grabbing the target.
    PointerMoved,
    /// Pointer button was pressed on the target.
    Pressed,
    /// Pointer button was released after pressing the target.
    Released,
    /// Pointer press and release completed on the same target.
    Clicked,
    /// A pointer drag crossed the movement threshold.
    DragStarted,
    /// A pointer drag moved after crossing the threshold.
    Dragged,
    /// A pointer drag ended.
    DragFinished,
    /// A pointer wheel or touchpad axis changed.
    Wheel,
    /// A key was pressed while the target held focus.
    KeyPressed,
    /// A key was released while the target held focus.
    KeyReleased,
    /// A touch contact began on the target.
    TouchPressed,
    /// A grabbed touch contact moved.
    TouchMoved,
    /// A grabbed touch contact ended.
    TouchReleased,
    /// A grabbed touch contact was cancelled.
    TouchCanceled,
    /// A drag from another application moved over a `DropArea`.
    ///
    /// A drag's arrival and departure reuse `on_entered` and `on_exited`: a
    /// `DropArea` is never under the pointer in the ordinary sense, so the two
    /// names are free to mean the drag's.
    DropMoved,
    /// A drag was let go over a `DropArea` that accepted it.
    Dropped,
    /// A text input's text was edited, by a key, a paste or a method.
    TextChanged,
    /// Enter was pressed in a single-line text input, or Ctrl+Enter in a
    /// multi-line one.
    Accepted,
    /// Escape was pressed in a text input.
    Escape,
    /// A text input gained or lost the keyboard.
    FocusChanged,
    /// A link in a text's runs was clicked.
    LinkActivated,
    /// A press was held still on the target long enough.
    LongPressed,
    /// A second click landed on the target soon after the first.
    DoubleClicked,
    /// A press was flung across the target and let go while moving.
    Swiped,
    /// Two touches on the target moved apart or together.
    Pinched,
    /// A touch began at a surface's edge and moved in (on its root).
    EdgeSwiped,
    /// An owned touch drag: begin, update, end or cancel, with displacement
    /// and recent velocity. Returning false from begin declines ownership.
    Panned,
    /// A continuous drag beginning at the surface edge (on its root).
    EdgePanned,
    /// A screen reader asked something of the node (`api_accessible.rs`).
    AccessibleAction,
}

/// Every event a configuration can handle, and the property it writes.
///
/// One table, read in both directions. It used to be written out twice — once
/// each way, in two files — with nothing keeping the two in step, so an event
/// added to one and forgotten in the other would either be unbindable or
/// unnameable, and nothing would say which.
pub const EVENT_PROPERTIES: &[(UiEvent, &str)] = &[
    (UiEvent::PointerEntered, "on_entered"),
    (UiEvent::PointerExited, "on_exited"),
    (UiEvent::PointerMoved, "on_position_changed"),
    (UiEvent::Pressed, "on_pressed"),
    (UiEvent::Released, "on_released"),
    (UiEvent::Clicked, "on_clicked"),
    (UiEvent::DragStarted, "on_drag_started"),
    (UiEvent::Dragged, "on_dragged"),
    (UiEvent::DragFinished, "on_drag_finished"),
    (UiEvent::Wheel, "on_wheel"),
    (UiEvent::KeyPressed, "on_key_pressed"),
    (UiEvent::KeyReleased, "on_key_released"),
    (UiEvent::TouchPressed, "on_touch_pressed"),
    (UiEvent::TouchMoved, "on_touch_moved"),
    (UiEvent::TouchReleased, "on_touch_released"),
    (UiEvent::TouchCanceled, "on_touch_canceled"),
    (UiEvent::DropMoved, "on_moved"),
    (UiEvent::Dropped, "on_dropped"),
    (UiEvent::TextChanged, "on_text_changed"),
    (UiEvent::Accepted, "on_accepted"),
    (UiEvent::Escape, "on_escape"),
    (UiEvent::FocusChanged, "on_focus_changed"),
    (UiEvent::LinkActivated, "on_link"),
    (UiEvent::LongPressed, "on_long_pressed"),
    (UiEvent::DoubleClicked, "on_double_clicked"),
    (UiEvent::Swiped, "on_swiped"),
    (UiEvent::Pinched, "on_pinched"),
    (UiEvent::EdgeSwiped, "on_edge_swiped"),
    (UiEvent::Panned, "on_panned"),
    (UiEvent::EdgePanned, "on_edge_panned"),
    (UiEvent::AccessibleAction, "on_accessible_action"),
];

impl UiEvent {
    /// The node property a handler for this event is set as.
    pub fn property(self) -> &'static str {
        EVENT_PROPERTIES
            .iter()
            .find(|(event, _)| *event == self)
            .map_or("on_unknown", |(_, property)| *property)
    }
}

/// One pointer or touch position, in both spaces a Lua handler may want.
///
/// `surface_x`/`surface_y` are the coordinates the compositor delivered, shared
/// by every node on the surface — the space `Layout::hit_test` is queried in.
/// `local_x`/`local_y` are the same point inside the node whose handler runs:
/// `0.0` at its own top-left corner, its width and height at the far edges,
/// with every ancestor offset and transform removed. A handler that wants a
/// fraction of its own extent divides the local pair and nothing else.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EventPoint {
    /// Pointer x in surface space.
    pub surface_x: f64,
    /// Pointer y in surface space.
    pub surface_y: f64,
    /// Pointer x inside the handling node.
    pub local_x: f64,
    /// Pointer y inside the handling node.
    pub local_y: f64,
    /// The Linux button code of a press, release or click, when there was
    /// one; handlers get its name as a fifth argument (nil without one) and
    /// the held modifiers as a sixth.
    pub button: Option<u32>,
}

impl EventPoint {
    /// Builds a point from surface coordinates and their node-local pair.
    pub fn new(surface: (f64, f64), local: (f64, f64)) -> Self {
        Self {
            surface_x: surface.0,
            surface_y: surface.1,
            local_x: local.0,
            local_y: local.1,
            button: None,
        }
    }

    /// The same point, from a press of `button` (a Linux input code).
    pub fn with_button(mut self, button: u32) -> Self {
        self.button = Some(button);
        self
    }

    /// `left`, `right`, `middle`, `back`, `forward`, or the code as text.
    pub fn button_name(button: u32) -> String {
        match button {
            0x110 => "left".to_owned(),
            0x111 => "right".to_owned(),
            0x112 => "middle".to_owned(),
            0x113 => "back".to_owned(),
            0x114 => "forward".to_owned(),
            other => other.to_string(),
        }
    }

    /// What a handler is called with: the four coordinates, the button's
    /// name (nil without one), and `held`, the modifiers held.
    pub fn args(self, held: IpcValue) -> Vec<IpcValue> {
        let mut args = vec![
            IpcValue::Number(self.surface_x),
            IpcValue::Number(self.surface_y),
            IpcValue::Number(self.local_x),
            IpcValue::Number(self.local_y),
        ];
        args.push(match self.button {
            Some(button) => IpcValue::String(Self::button_name(button)),
            None => IpcValue::Nil,
        });
        args.push(held);
        args
    }
}

/// The nodes something has read `contains_pointer` of, and what the host
/// last said about each.
#[derive(Default)]
pub struct PointerWatch {
    inside: std::collections::HashMap<NodeHandle, bool>,
    /// Read for the first time since the host last asked.
    fresh: Vec<NodeHandle>,
}

impl PointerWatch {
    /// What is known of `node`: whether the pointer is inside it. The first
    /// read starts watching it (outside until the host says otherwise).
    pub fn read(&mut self, node: NodeHandle) -> bool {
        match self.inside.get(&node) {
            Some(inside) => *inside,
            None => {
                self.inside.insert(node, false);
                self.fresh.push(node);
                false
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inside.is_empty()
    }

    /// Every watched node.
    pub fn watched(&self) -> Vec<NodeHandle> {
        self.inside.keys().copied().collect()
    }

    /// The nodes first read since this was last asked.
    pub fn take_fresh(&mut self) -> Vec<NodeHandle> {
        std::mem::take(&mut self.fresh)
    }

    pub fn has_fresh(&self) -> bool {
        !self.fresh.is_empty()
    }

    /// Records the host's answers; returns the watched nodes whose answer
    /// changed. A node nothing has read is ignored.
    pub fn answer(&mut self, answers: &[(NodeHandle, bool)]) -> Vec<NodeHandle> {
        let mut changed = Vec::new();
        for &(node, inside) in answers {
            match self.inside.get_mut(&node) {
                Some(value) if *value != inside => *value = inside,
                _ => continue,
            }
            changed.push(node);
        }
        changed
    }

    /// Forgets a node that is gone.
    pub fn forget(&mut self, node: NodeHandle) {
        self.inside.remove(&node);
        self.fresh.retain(|fresh| *fresh != node);
    }
}

/// Which modifier keys were held with a key.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KeyModifiers {
    /// Either Control.
    pub ctrl: bool,
    /// Either Shift.
    pub shift: bool,
    /// Either Alt.
    pub alt: bool,
    /// The Super (logo) key.
    pub logo: bool,
}

impl KeyModifiers {
    /// The held modifiers as a configuration reads them: `"ctrl+shift"`, or
    /// an empty string for none.
    pub fn name(self) -> String {
        [
            (self.ctrl, "ctrl"),
            (self.shift, "shift"),
            (self.alt, "alt"),
            (self.logo, "super"),
        ]
        .into_iter()
        .filter_map(|(held, name)| held.then_some(name))
        .collect::<Vec<_>>()
        .join("+")
    }
}
