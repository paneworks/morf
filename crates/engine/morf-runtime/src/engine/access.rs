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
    /// The verbs answered over IPC.
    ipc, ipc_mut: crate::ipc::IpcVerbs;
    /// Each `ui.Layout` container's functions, and the linked texts.
    layout_hooks, layout_hooks_mut: crate::layout::LayoutHooks;
    /// Each list model a binding has read, with its revision signal.
    model_revisions, model_revisions_mut: crate::models::ModelRevisions;
    /// The shell's log lines.
    logs, logs_mut: crate::log::Log;
    /// `morf.clock` and its coarser grains.
    clocks, clocks_mut: crate::wake::Clocks;
}
