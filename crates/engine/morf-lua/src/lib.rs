//! Sandboxed execution of morf configuration code.

// Each `api/` area keeps its main table in a file named for it
// (`api/color/color.rs`), as PLAN.md lays the binding out.
#![allow(clippy::module_inception)]

mod api;
mod runtime;
mod value;
mod vm;

pub use self::runtime::keys;
pub use self::vm::arguments;
pub use self::vm::profile;
pub use self::vm::state_cell::{StateRef, StateRefMut};

pub use self::api::desktop::gamma::GammaRequest;
pub use self::api::input::focus::FocusReason;
pub use self::api::io::broadcast::set_shell_socket;
#[cfg(feature = "kit")]
pub use self::api::kit::register as register_kit;
pub use self::runtime::events::*;
pub use self::runtime::host_types::*;
pub use self::runtime::layout::SettledLayout;
pub use self::runtime::session_lock::SessionLockState;
pub use self::runtime::text_inputs::KeyModifiers;
pub use self::runtime::wake::{ClockPrecision, DeadlineCause};
pub use self::vm::extensions::{Extension, NodesRemoved, register_extension};
pub use self::vm::harness::HostFunction;
pub use self::vm::loader::{project_library, runtimepath_roots};
pub use self::vm::permissions::{Approvals, ModuleDeclaration, fingerprint as module_fingerprint};
pub use self::vm::types::*;
pub use self::vm::types_gen::generate_types;
pub use morf_value::IpcValue;

#[cfg(test)]
mod internal_tests;
