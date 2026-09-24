//! Key repeat, done by the client as Wayland asks.
//!
//! A compositor sends one press and one release however long a key is held;
//! `wl_keyboard.repeat_info` says how fast the client should repeat it
//! itself. Holding Backspace in a field, an arrow in a list or a key in a
//! terminal only repeats if this does it: the toolkit's own repeat needs a
//! calloop loop, which the shell's loop is not.

use std::time::{Duration, Instant};

use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym, RepeatInfo};

/// How long a repeat may fall behind before it is caught up in one go
/// rather than key by key: a stalled frame must not come back as a burst.
const MOST_BEHIND: u32 = 3;

#[derive(Debug)]
pub(crate) struct KeyRepeat {
    /// Delay before the first repeat and the gap between repeats; `None`
    /// when the compositor turned repeat off.
    timing: Option<(Duration, Duration)>,
    held: Option<(KeyEvent, Instant)>,
}

impl Default for KeyRepeat {
    /// Until the compositor says (it does as soon as there is a keyboard),
    /// the commonest setting: 600 ms, then 25 a second.
    fn default() -> Self {
        Self {
            timing: Some((Duration::from_millis(600), Duration::from_millis(40))),
            held: None,
        }
    }
}

impl KeyRepeat {
    pub(crate) fn set_info(&mut self, info: RepeatInfo) {
        self.timing = match info {
            RepeatInfo::Repeat { rate, delay } => Some((
                Duration::from_millis(u64::from(delay)),
                Duration::from_micros(1_000_000 / u64::from(rate.get())),
            )),
            RepeatInfo::Disable => None,
        };
        if self.timing.is_none() {
            self.held = None;
        }
    }

    /// A key went down: it repeats from now, in place of any other.
    /// Modifiers and lock keys never repeat.
    pub(crate) fn press(&mut self, event: &KeyEvent, now: Instant) {
        self.held = match self.timing {
            Some((delay, _)) if !never_repeats(event.keysym) => Some((event.clone(), now + delay)),
            _ => None,
        };
    }

    /// A key came up: if it was the one repeating, the repeat stops.
    pub(crate) fn release(&mut self, event: &KeyEvent) {
        if self
            .held
            .as_ref()
            .is_some_and(|(held, _)| held.raw_code == event.raw_code)
        {
            self.held = None;
        }
    }

    /// Focus went, or the keyboard did: nothing is held any more.
    pub(crate) fn stop(&mut self) {
        self.held = None;
    }

    /// When the next repeat is due.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.held.as_ref().map(|(_, at)| *at)
    }

    /// The repeats due by `now`, each as the key it repeats.
    pub(crate) fn due(&mut self, now: Instant) -> Vec<KeyEvent> {
        let Some((_, gap)) = self.timing else {
            return Vec::new();
        };
        let Some((event, at)) = self.held.as_mut() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        while *at <= now {
            out.push(event.clone());
            *at += gap;
            if out.len() as u32 >= MOST_BEHIND {
                *at = (*at).max(now + gap);
                break;
            }
        }
        out
    }
}

fn never_repeats(keysym: Keysym) -> bool {
    keysym.is_modifier_key()
        || matches!(
            keysym,
            Keysym::Caps_Lock | Keysym::Num_Lock | Keysym::Scroll_Lock | Keysym::Shift_Lock
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    fn key(keysym: Keysym, raw_code: u32) -> KeyEvent {
        KeyEvent {
            time: 0,
            raw_code,
            keysym,
            utf8: None,
        }
    }

    #[test]
    fn a_held_key_repeats_after_the_delay_at_the_rate() {
        let mut repeat = KeyRepeat::default();
        repeat.set_info(RepeatInfo::Repeat {
            rate: NonZeroU32::new(10).unwrap(),
            delay: 500,
        });
        let start = Instant::now();
        repeat.press(&key(Keysym::BackSpace, 14), start);
        assert!(repeat.due(start + Duration::from_millis(499)).is_empty());
        assert_eq!(repeat.due(start + Duration::from_millis(500)).len(), 1);
        assert_eq!(
            repeat.deadline(),
            Some(start + Duration::from_millis(600)),
            "then one every tenth of a second"
        );
        assert_eq!(repeat.due(start + Duration::from_millis(700)).len(), 2);
        repeat.release(&key(Keysym::BackSpace, 14));
        assert_eq!(repeat.deadline(), None);
        assert!(repeat.due(start + Duration::from_secs(5)).is_empty());
    }

    #[test]
    fn the_last_key_pressed_is_the_one_that_repeats() {
        let mut repeat = KeyRepeat::default();
        let start = Instant::now();
        repeat.press(&key(Keysym::a, 30), start);
        repeat.press(&key(Keysym::b, 48), start);
        // Letting go of the first leaves the second repeating.
        repeat.release(&key(Keysym::a, 30));
        let due = repeat.due(start + Duration::from_secs(1));
        assert!(!due.is_empty());
        assert!(due.iter().all(|event| event.keysym == Keysym::b));
    }

    #[test]
    fn modifiers_do_not_repeat_and_a_stall_is_not_a_burst() {
        let mut repeat = KeyRepeat::default();
        let start = Instant::now();
        repeat.press(&key(Keysym::Shift_L, 42), start);
        assert_eq!(repeat.deadline(), None);
        repeat.press(&key(Keysym::Left, 105), start);
        assert!(repeat.due(start + Duration::from_secs(10)).len() <= MOST_BEHIND as usize);
        repeat.set_info(RepeatInfo::Disable);
        assert_eq!(repeat.deadline(), None);
    }
}
