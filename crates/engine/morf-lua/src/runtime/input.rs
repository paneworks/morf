use morf_scene::{NodeHandle, Value as SceneValue};

use crate::{events::*, surface_types::*, types::*};

/// The held modifiers as a pointer handler is told them: `"ctrl+shift"`.
pub(crate) use morf_runtime::events::held;

impl Runtime {
    /// Says which modifiers the seat holds, for the pointer handlers that
    /// follow: a press, a drag or a wheel turn is told them as its last
    /// argument (a Shift-click extends, Ctrl with the wheel zooms).
    pub fn set_held_modifiers(&self, modifiers: crate::KeyModifiers) {
        set_held(modifiers);
    }

    /// Runs one key with no modifiers held; see [`Runtime::dispatch_key`].
    pub fn dispatch_key_event(
        &mut self,
        node: NodeHandle,
        keysym: u32,
        text: Option<&str>,
    ) -> bool {
        self.dispatch_key(node, keysym, text, crate::KeyModifiers::default())
    }

    /// Dispatches one touch event with contact identity and both coordinate
    /// spaces: `(id, surface_x, surface_y, local_x, local_y)`.
    pub fn dispatch_touch_event(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        id: i32,
        point: EventPoint,
    ) -> bool {
        if !matches!(
            event,
            UiEvent::TouchPressed
                | UiEvent::TouchMoved
                | UiEvent::TouchReleased
                | UiEvent::TouchCanceled
        ) {
            return false;
        }
        if event == UiEvent::TouchPressed {
            self.reactive.borrow_mut().gestures.touch();
        }
        self.dispatch_ui_event_with_args(node, event, &args::touch_args(id, point))
    }

    /// Dispatches one pointer event, whatever kind it is.
    ///
    /// The single entry a host uses. A press, a release and a click carry only
    /// a position; a motion or a drag also carries how far the pointer has
    /// travelled since the press. Routing on the event here rather than at the
    /// call site is deliberate: when the two were separate public methods, a
    /// host that reached for the wrong one got `false` and silence, and every
    /// click in the shell was dropped for exactly that reason. There is now no
    /// wrong one to reach for.
    pub fn dispatch_pointer(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        point: EventPoint,
        delta: (f64, f64),
    ) -> bool {
        // A long press, a double click and a swipe are made of these
        // (`gestures.rs`); a click a long press took goes nowhere.
        if !self.watch_gesture(node, event, point) {
            return false;
        }
        let handled = self.deliver_pointer(node, event, point, delta);
        handled | self.finish_gesture(node, event, point)
    }

    fn deliver_pointer(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        point: EventPoint,
        delta: (f64, f64),
    ) -> bool {
        // A terminal answers the pointer itself: it takes the keyboard on a
        // press, and a program that asked for the pointer is sent it.
        if self.is_terminal(node) {
            let action = match event {
                UiEvent::Pressed => Some(morf_terminal::MouseAction::Press),
                UiEvent::Released => Some(morf_terminal::MouseAction::Release),
                UiEvent::PointerMoved | UiEvent::Dragged | UiEvent::DragStarted => {
                    Some(morf_terminal::MouseAction::Motion)
                }
                _ => None,
            };
            let changed = action.is_some_and(|action| {
                self.terminal_pointer(node, action, point.button, (point.local_x, point.local_y))
            });
            return changed | self.dispatch_pointer_handler(node, event, point, delta);
        }
        // A text input answers the pointer itself — a caret where it was
        // pressed, a selection where it is dragged — and then its own
        // handlers, if the configuration gave it any, hear it as usual.
        let edited = self.is_text_input(node) && {
            let local = (point.local_x, point.local_y);
            {
                let mut state = self.reactive.borrow_mut();
                match event {
                    UiEvent::Pressed => crate::text_inputs::press(&mut *state, node, local),
                    UiEvent::PointerMoved | UiEvent::Dragged | UiEvent::DragStarted => {
                        crate::text_inputs::drag(&mut *state, node, local);
                    }
                    UiEvent::Released | UiEvent::DragFinished => {
                        crate::text_inputs::release(&mut *state, node);
                    }
                    _ => {}
                }
            }
            self.finish_text_input_work();
            matches!(
                event,
                UiEvent::Pressed | UiEvent::PointerMoved | UiEvent::Dragged | UiEvent::Released
            )
        };
        // A click on a link in a text's runs is the link's.
        if event == UiEvent::Clicked
            && self.reactive.borrow().scene.element(node).ok() == Some(morf_scene::Element::Text)
        {
            let href = morf_layout::link_at(
                &self.reactive.borrow().scene,
                node,
                point.local_x,
                point.local_y,
            );
            if let Some(href) = href {
                return self.dispatch_ui_event_with_args(
                    node,
                    UiEvent::LinkActivated,
                    &[IpcValue::String(href)],
                );
            }
        }
        edited | self.dispatch_pointer_handler(node, event, point, delta)
    }

    fn dispatch_pointer_handler(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        point: EventPoint,
        delta: (f64, f64),
    ) -> bool {
        match event {
            UiEvent::Pressed | UiEvent::Released | UiEvent::Clicked => {
                self.dispatch_button_event(node, event, point)
            }
            UiEvent::PointerMoved
            | UiEvent::DragStarted
            | UiEvent::Dragged
            | UiEvent::DragFinished => self.dispatch_pointer_event(node, event, point, delta),
            _ => false,
        }
    }

    /// Dispatches a pointer button event as `(surface_x, surface_y, local_x,
    /// local_y)`, the position the button was pressed or released at.
    pub(crate) fn dispatch_button_event(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        point: EventPoint,
    ) -> bool {
        if !matches!(
            event,
            UiEvent::Pressed | UiEvent::Released | UiEvent::Clicked
        ) {
            return false;
        }
        self.dispatch_ui_event_with_args(node, event, &args::button_args(point))
    }

    /// Dispatches pointer coordinates and displacement to a movement handler as
    /// `(surface_x, surface_y, delta_x, delta_y, local_x, local_y)`.
    ///
    /// The displacement stays measured in surface space: it is the distance the
    /// pointer has travelled since the press, and a drag is free to leave the
    /// node it started on — in which case the local pair runs past the node's
    /// own bounds rather than clamping.
    pub(crate) fn dispatch_pointer_event(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        point: EventPoint,
        delta: (f64, f64),
    ) -> bool {
        if !matches!(
            event,
            UiEvent::PointerMoved | UiEvent::DragStarted | UiEvent::Dragged | UiEvent::DragFinished
        ) {
            return false;
        }
        self.dispatch_ui_event_with_args(node, event, &args::motion_args(point, delta))
    }

    /// Dispatches one wheel or touchpad-axis event to a MouseArea as
    /// `(surface_x, surface_y, pixel_x, pixel_y, step_x, step_y, local_x,
    /// local_y)`.
    pub fn dispatch_wheel_event(
        &mut self,
        node: NodeHandle,
        point: EventPoint,
        pixels: (f64, f64),
        steps: (i32, i32),
    ) -> bool {
        if self.is_terminal(node) {
            return self.terminal_wheel(node, (point.local_x, point.local_y), pixels.1, steps.1);
        }
        self.dispatch_ui_event_with_args(
            node,
            UiEvent::Wheel,
            &args::wheel_args(point, pixels, steps),
        )
    }

    /// Whether a wheel turn over `node` stops there: a MouseArea (or text
    /// input) that has an `on_wheel` handler, or any Flickable. Everything
    /// else lets the wheel bubble on to what is beneath it, so a button on a
    /// scrolling page does not swallow the page's scroll.
    ///
    /// A host hit-testing a Flickable should also ask whether it has room to
    /// move; see [`Runtime::scroll_flickable`].
    pub fn takes_wheel(&self, node: NodeHandle) -> bool {
        self.reactive.borrow().engine.takes_wheel(node)
    }

    /// Scrolls a Flickable by a wheel's pixel delta, keeping its
    /// `content_x`/`content_y` inside `0..=max`. Returns whether it moved.
    ///
    /// `max` is how far the content can scroll on each axis, the content's
    /// extent less the viewport's, which only the layout knows.
    pub fn scroll_flickable(
        &mut self,
        node: NodeHandle,
        pixels: (f64, f64),
        max: (f64, f64),
    ) -> bool {
        let mut moved = false;
        {
            let mut state = self.reactive.borrow_mut();
            for (property, next) in routing::flickable_scroll(&state.scene, node, pixels, max) {
                if crate::scene_bindings::assign_scene_property(
                    &mut state,
                    node,
                    property,
                    SceneValue::Number(next),
                )
                .is_ok()
                {
                    moved = true;
                }
            }
        }
        if moved {
            self.flush_after_event();
        }
        moved
    }

    /// Returns whether a MouseArea accepts one Linux input button code.
    pub fn accepts_pointer_button(&self, node: NodeHandle, button: u32) -> bool {
        self.reactive
            .borrow()
            .engine
            .accepts_pointer_button(node, button)
    }
}
