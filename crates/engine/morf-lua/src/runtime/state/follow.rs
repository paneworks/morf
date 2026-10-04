//! The state motion runs in (`morf_runtime::animation`): its scene, and a
//! property write that goes through the bindings like any other.

use super::*;

impl morf_runtime::animation::AnimationHost for ReactiveState {
    fn animation(&mut self) -> &mut morf_runtime::animation::Animation {
        &mut self.animation
    }

    fn scene(&mut self) -> &mut Scene {
        &mut self.scene
    }

    fn assign(
        &mut self,
        node: NodeHandle,
        property: &str,
        value: morf_scene::Value,
    ) -> Result<(), String> {
        crate::scene_bindings::assign_scene_property(self, node, property, value)
    }
}

/// Brings every following property up to its source. How many moved.
pub(crate) fn apply_follows(state: &mut ReactiveState) -> usize {
    morf_runtime::animation::follow::apply_follows(state)
}
