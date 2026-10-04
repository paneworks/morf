//! The modifier keys the seat last said were held. A seat has one keyboard,
//! so this is one value for every runtime on the thread.

use morf_value::IpcValue;

use super::KeyModifiers;

thread_local! {
    static HELD: std::cell::Cell<KeyModifiers> = std::cell::Cell::new(KeyModifiers::default());
}

/// Says which modifiers the seat holds, for the pointer handlers that follow.
pub fn set_held(modifiers: KeyModifiers) {
    HELD.with(|held| held.set(modifiers));
}

/// The held modifiers as a pointer handler is told them: `"ctrl+shift"`.
pub fn held() -> IpcValue {
    IpcValue::String(HELD.with(|held| held.get()).name())
}
