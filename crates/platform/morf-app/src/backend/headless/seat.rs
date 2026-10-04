//! A seat with nobody at it: pointer, keyboard and touch events made by a
//! script, with the state a compositor keeps between them.

use crate::{Event, KeyModifiers, WindowId};

/// What the seat remembers between events.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VirtualSeat {
    /// Where the pointer is, for a wheel or a release with no position.
    pub pointer: Option<(WindowId, f64, f64)>,
    /// The window a button was last pressed on: the one a compositor gives
    /// the keyboard to.
    pub keyboard: Option<WindowId>,
}

impl VirtualSeat {
    /// Keeps track of what `event` does to the seat, as a compositor would
    /// on sending it.
    pub fn observe(&mut self, event: &Event) {
        match event {
            Event::PointerMotion { surface, x, y }
            | Event::PointerButton { surface, x, y, .. }
            | Event::PointerAxis { surface, x, y, .. } => self.pointer = Some((*surface, *x, *y)),
            Event::PointerLeave { surface } => {
                if self.pointer.is_some_and(|(on, _, _)| on == *surface) {
                    self.pointer = None;
                }
            }
            _ => {}
        }
        if let Event::PointerButton { surface, pressed: true, .. } = event {
            self.keyboard = Some(*surface);
        }
    }

    /// A click: the pointer comes to `(x, y)` on `surface`, and `button`
    /// goes down and up there.
    pub fn click(
        surface: WindowId,
        (x, y): (f64, f64),
        button: u32,
        modifiers: KeyModifiers,
    ) -> [Event; 3] {
        let press = |pressed| Event::PointerButton { surface, button, pressed, x, y, modifiers };
        [Event::PointerMotion { surface, x, y }, press(true), press(false)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_gives_the_keyboard_and_a_leave_takes_the_pointer() {
        let mut seat = VirtualSeat::default();
        let window = WindowId::Layer(0);
        for event in VirtualSeat::click(window, (4.0, 5.0), 0x110, KeyModifiers::default()) {
            seat.observe(&event);
        }
        assert_eq!(seat.keyboard, Some(window));
        assert_eq!(seat.pointer, Some((window, 4.0, 5.0)));
        seat.observe(&Event::PointerLeave { surface: WindowId::Popup(1) });
        assert!(seat.pointer.is_some());
        seat.observe(&Event::PointerLeave { surface: window });
        assert_eq!(seat.pointer, None);
    }
}
