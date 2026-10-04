//! The windows a configuration has declared, as the host reads them: its
//! own surface's layer settings, every popup, toplevel and extra layer
//! surface, what each hears, and what changed since the host last looked.

use std::collections::HashMap;

use crate::handler::Handler;

use super::{
    LayerSurfaceConfig, ParentTransitionRequest, PopupNodeAnchor, WindowEvent, WindowSize,
    WindowSurfaceAction, WindowSurfaceConfig,
};

/// Every window the configuration declared, and what the host owes them.
#[derive(Default)]
pub struct Declarations {
    /// The shell's own surface (`morf.surface`).
    pub layer_surface: LayerSurfaceConfig,
    pub layer_surface_changed: bool,
    pub window_surfaces: HashMap<u64, WindowSurfaceConfig>,
    pub next_window_surface: u64,
    pub window_surfaces_changed: bool,
    /// Popups' and toplevels' configured sizes, and the signals reading them.
    pub window_sizes: HashMap<u64, WindowSize>,
    pub window_handlers: HashMap<(u64, WindowEvent), Handler>,
    /// What the shell's own surface hears.
    pub surface_handlers: HashMap<WindowEvent, Handler>,
    pub window_surface_actions: Vec<WindowSurfaceAction>,
    pub popup_node_anchors: HashMap<u64, PopupNodeAnchor>,
    pub parent_transitions: Vec<ParentTransitionRequest>,
}

impl Declarations {
    /// Every declared window, by id.
    pub fn configs(&self) -> Vec<WindowSurfaceConfig> {
        let mut surfaces = self.window_surfaces.values().cloned().collect::<Vec<_>>();
        surfaces.sort_by_key(|surface| surface.id);
        surfaces
    }

    /// Whether the set of windows changed since this was last asked.
    pub fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.window_surfaces_changed)
    }

    /// Whether the shell's own surface settings changed since last asked.
    pub fn take_layer_change(&mut self) -> bool {
        std::mem::take(&mut self.layer_surface_changed)
    }

    pub fn take_actions(&mut self) -> Vec<WindowSurfaceAction> {
        std::mem::take(&mut self.window_surface_actions)
    }

    pub fn take_parent_transitions(&mut self) -> Vec<ParentTransitionRequest> {
        std::mem::take(&mut self.parent_transitions)
    }

    /// Shows or hides window `id`. Returns whether that changed anything.
    pub fn set_visible(&mut self, id: u64, visible: bool) -> bool {
        let Some(surface) = self.window_surfaces.get_mut(&id) else {
            return false;
        };
        if surface.visible == visible {
            return false;
        }
        surface.visible = visible;
        self.window_surfaces_changed = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows::WindowSurfaceKind;
    use morf_scene::Scene;

    fn declare(declarations: &mut Declarations, id: u64) {
        let root = Scene::new().create(morf_scene::Element::Item);
        declarations.window_surfaces.insert(
            id,
            WindowSurfaceConfig {
                id,
                root,
                visible: false,
                updates_enabled: true,
                kind: WindowSurfaceKind::Layer(LayerSurfaceConfig::default()),
            },
        );
    }

    #[test]
    fn windows_are_listed_by_id_and_a_change_is_heard_once() {
        let mut declarations = Declarations::default();
        declare(&mut declarations, 7);
        declare(&mut declarations, 2);
        let ids = declarations
            .configs()
            .iter()
            .map(|surface| surface.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![2, 7]);
        assert!(!declarations.take_change());
        assert!(declarations.set_visible(7, true));
        assert!(!declarations.set_visible(7, true));
        assert!(!declarations.set_visible(99, true));
        assert!(declarations.take_change());
        assert!(!declarations.take_change());
    }

    #[test]
    fn actions_and_layer_changes_are_taken_once() {
        let mut declarations = Declarations::default();
        declarations
            .window_surface_actions
            .push(WindowSurfaceAction::Move { id: 1 });
        declarations.layer_surface_changed = true;
        assert_eq!(declarations.take_actions().len(), 1);
        assert!(declarations.take_actions().is_empty());
        assert!(declarations.take_layer_change());
        assert!(!declarations.take_layer_change());
        assert!(declarations.take_parent_transitions().is_empty());
    }
}
