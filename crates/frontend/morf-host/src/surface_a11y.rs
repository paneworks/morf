//! Each surface's accessible tree, for a screen reader (the `a11y`
//! feature: AT-SPI through AccessKit, `morf_app::accesskit`).
//!
//! Every surface gets an adapter as it appears; none builds a tree until a
//! screen reader asks for one. While one is wanted, the loop's turn after
//! the paints rebuilds the tree when the scene has moved on (or the layout
//! under it, at most a few times a second) and sends what changed. A screen
//! reader's requests are carried out through `Runtime::accessible_action`,
//! as the node's own keys would.

#[cfg(not(feature = "a11y"))]
use {crate::surfaces::SurfaceEventState, morf_lua::Runtime, morf_scene::NodeHandle};

#[cfg(feature = "a11y")]
mod live {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use morf_lua::Runtime;
use morf_value::IpcValue;
    use morf_scene::NodeHandle;
    use morf_app::accesskit::{Accessibility, RequestKind};

    use crate::surfaces::SurfaceEventState;

    /// How stale a tree may get on layout alone (things moving with no
    /// property written): bounds a screen reader reads are this fresh.
    const LAYOUT_STALE: Duration = Duration::from_millis(250);

    struct Adapter {
        accessibility: Accessibility,
        built_revision: Option<u64>,
        built_at: Option<Instant>,
    }

    #[derive(Default)]
    pub struct A11ySurfaces {
        adapters: HashMap<NodeHandle, Adapter>,
    }

    impl A11ySurfaces {
        pub fn window_focus(&mut self, root: NodeHandle, focused: bool) {
            if let Some(adapter) = self.adapters.get_mut(&root) {
                adapter.accessibility.set_window_focused(focused);
            }
        }

        pub fn turn(
            &mut self,
            runtime: &mut Runtime,
            state: &SurfaceEventState,
            name: &str,
            painted: bool,
        ) -> bool {
            if std::env::var_os("MORF_NO_A11Y").is_some() {
                return false;
            }
            let mut roots: Vec<NodeHandle> = vec![state.primary_root];
            roots.extend(
                state
                    .popup_surfaces
                    .values()
                    .chain(state.floating_surfaces.values())
                    .chain(state.layer_surfaces.values())
                    .map(|s| s.root),
            );
            self.adapters.retain(|root, _| roots.contains(root));
            let mut acted = false;
            for root in &roots {
                let adapter = self.adapters.entry(*root).or_insert_with(|| Adapter {
                    accessibility: Accessibility::new(Arc::new(morf_io::wake_all)),
                    built_revision: None,
                    built_at: None,
                });
                for request in adapter.accessibility.take_requests() {
                    let (action, value) = match request.kind {
                        RequestKind::Focus => ("focus", None),
                        RequestKind::Click => ("click", None),
                        RequestKind::Increment => ("increment", None),
                        RequestKind::Decrement => ("decrement", None),
                        RequestKind::Expand => ("expand", None),
                        RequestKind::Collapse => ("collapse", None),
                        RequestKind::ScrollIntoView => ("scroll_into_view", None),
                        RequestKind::SetNumber(n) => ("set_value", Some(IpcValue::Number(n))),
                        RequestKind::SetText(t) => ("set_value", Some(IpcValue::String(t))),
                    };
                    let node = morf_scene::NodeHandle::from_bits(request.node);
                    acted |= runtime.accessible_action(*root, node, action, value);
                }
            }
            for root in &roots {
                let Some(adapter) = self.adapters.get_mut(root) else { continue };
                if !adapter.accessibility.wants_tree() {
                    adapter.built_revision = None;
                    continue;
                }
                let revision = runtime.scene_revision();
                let fresh = adapter.accessibility.is_fresh() || adapter.built_revision.is_none();
                let stale = painted && adapter.built_at.is_none_or(|at| at.elapsed() >= LAYOUT_STALE);
                if !fresh && !stale && adapter.built_revision == Some(revision) {
                    continue;
                }
                let layout = if *root == state.primary_root {
                    Some(&*state.layout)
                } else {
                    state
                        .popup_surfaces
                        .values()
                        .chain(state.floating_surfaces.values())
                        .chain(state.layer_surfaces.values())
                        .find(|s| s.root == *root)
                        .and_then(|s| s.layout.as_deref())
                };
                let Some(layout) = layout else { continue };
                let nodes = {
                    let scene = runtime.scene();
                    scene.accessible_tree(*root, "window", name, &|node| {
                        layout.surface_rect(&scene, node).map(|g| (g.x, g.y, g.width, g.height))
                    })
                };
                adapter.accessibility.update(&nodes);
                adapter.built_revision = Some(revision);
                adapter.built_at = Some(Instant::now());
            }
            acted
        }
    }
}

#[cfg(feature = "a11y")]
pub use live::A11ySurfaces;

/// With no `a11y` feature: nothing, at no cost.
#[cfg(not(feature = "a11y"))]
#[derive(Default)]
pub struct A11ySurfaces;

#[cfg(not(feature = "a11y"))]
impl A11ySurfaces {
    pub fn window_focus(&mut self, _root: NodeHandle, _focused: bool) {}

    pub fn turn(
        &mut self,
        _runtime: &mut Runtime,
        _state: &SurfaceEventState,
        _name: &str,
        _painted: bool,
    ) -> bool {
        false
    }
}
