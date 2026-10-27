use morf_scene::NodeHandle;

use crate::{runtime::host_types::*, runtime::state::*};

/// Registers one window surface and returns the id it was given.
///
/// The id allocation, the insert and the change flag were written out once per
/// surface kind — three copies of the same six lines, differing only in which
/// `WindowSurfaceKind` they wrapped — so adding a field to the record meant
/// finding all three.
pub(crate) fn register_window_surface(
    state: &mut ReactiveState,
    root: NodeHandle,
    visible: bool,
    updates_enabled: bool,
    kind: WindowSurfaceKind,
) -> u64 {
    let id = state.windows().next_window_surface;
    state.windows_mut().next_window_surface = id.wrapping_add(1);
    state.windows_mut().window_surfaces.insert(
        id,
        WindowSurfaceConfig {
            id,
            root,
            visible,
            updates_enabled,
            kind,
        },
    );
    state.windows_mut().window_surfaces_changed = true;
    id
}
