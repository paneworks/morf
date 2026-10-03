//! Widget archetypes: behaviour with no look.
//!
//! An archetype owns a control's state, its response to the pointer and the
//! keyboard, and the slots a skin fills; a theme's skin, in Lua, draws it.
//! The engine does not know this crate: [`register`] adds `morf.kit.native`
//! to every runtime made afterwards, and `library/lib/kit/` builds controls
//! on it. A configuration that never requires the kit pays nothing for it.
//!
//! Each archetype is a state machine ([`Archetype`]): it is sent events --
//! `"pressed"`, `"key"`, `"focus"` -- and answers with [`Effects`]: the
//! state that changed and the signals to raise. The Lua side applies the
//! state to the table a skin reads and calls the configuration's handlers.

mod collection;
mod control;
mod disclosure;
mod drag;
mod group;
mod module;
mod navigation;
mod plane;
mod popup;
mod press;
mod range;
mod scroll;
mod selection;
mod slots;
mod text_field;
mod tokens;
mod value;

pub use control::{ControlState, implicit_size};
pub use module::install;
pub use slots::{ARCHETYPES, slots_of};
pub use tokens::merge_tokens;

use morf_lua::IpcValue;

/// What an archetype answers an event with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    /// State fields that changed, with their new values.
    pub changed: Vec<(String, IpcValue)>,
    /// Signals to raise, in order, each with its arguments.
    pub signals: Vec<(String, Vec<IpcValue>)>,
    /// Whether a key sent was used; one that was not goes on to whatever
    /// around the control takes keys.
    pub handled: bool,
    /// What the event did to other controls -- the rest of an exclusive
    /// group -- by id.
    pub others: Vec<(i64, Effects)>,
}

impl Effects {
    /// Records a field's new value.
    pub fn set(&mut self, field: &str, value: impl Into<IpcValue>) {
        let value = value.into();
        match self.changed.iter_mut().find(|(name, _)| name == field) {
            Some(entry) => entry.1 = value,
            None => self.changed.push((field.to_owned(), value)),
        }
    }

    /// Raises a signal.
    pub fn raise(&mut self, signal: &str, arguments: Vec<IpcValue>) {
        self.signals.push((signal.to_owned(), arguments));
    }

    /// Takes in another set of effects after these.
    pub fn extend(&mut self, other: Effects) {
        for (field, value) in other.changed {
            self.set(&field, value);
        }
        self.signals.extend(other.signals);
        self.handled |= other.handled;
        self.others.extend(other.others);
    }

    /// Marks a key as used.
    pub fn handled(mut self) -> Self {
        self.handled = true;
        self
    }
}

/// A behaviour with no look.
pub trait Archetype {
    /// Its name: `"Control"`, `"Press"`, ...
    fn name(&self) -> &'static str;
    /// Every state field and its value, for the table a skin reads.
    fn state(&self) -> Vec<(String, IpcValue)>;
    /// Takes one event and answers what it changed.
    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String>;
    /// Takes a setting the configuration wrote (`checked`, `value`, ...).
    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String>;
    /// The exclusive group it belongs to, if any: the registry keeps one
    /// checked at a time across its members.
    fn group(&self) -> Option<group::Membership> {
        None
    }
    /// Lets the registry change it as a member of its group.
    fn as_member(&mut self) -> Option<&mut dyn group::Member> {
        None
    }
}

/// Registers the kit with the engine: every runtime made afterwards can
/// `require("morf.kit.native")`.
pub fn register() {
    morf_lua::register_extension(install);
}
