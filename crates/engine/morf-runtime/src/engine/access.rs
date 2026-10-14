//! Each subsystem of the engine, reached by reference.

use super::Engine;

/// A shared and an exclusive accessor for each subsystem field.
macro_rules! subsystems {
    ($($(#[$doc:meta])* $field:ident, $field_mut:ident: $ty:ty;)*) => {
        impl Engine {
            $(
                $(#[$doc])*
                pub fn $field(&self) -> &$ty {
                    &self.$field
                }

                $(#[$doc])*
                pub fn $field_mut(&mut self) -> &mut $ty {
                    &mut self.$field
                }
            )*
        }
    };
}

subsystems! {
    /// Each node's declared states.
    states, states_mut: crate::states::States;
}
