//! What a layer surface takes from the compositor: its opaque region, the
//! keyboard focus it asks for, and where it accepts input.

use morf_value::region::Region;

use crate::backend::wayland::{state_types::*, surface_types::*};

use super::layer_interactivity;

impl LayerClient {
    /// Tells the compositor the whole surface is opaque, or stops claiming so.
    ///
    /// A hint, and one worth giving: a compositor blends every pixel of a
    /// surface it cannot prove opaque, and a full-width bar is a few hundred
    /// thousand of them every frame. Sized to the surface's current logical
    /// size, so it has to be re-applied after a configure.
    pub fn set_layer_opaque(&self, id: u64, opaque: bool) {
        let Some(surface) = self.layer_surface(id) else {
            return;
        };
        if !opaque {
            surface.set_opaque_region(None);
            return;
        }
        let Some((width, height)) = self.layer_logical_size(id) else {
            return;
        };
        let qh = self.queue.handle();
        let region = self.state.compositor.wl_compositor().create_region(&qh, ());
        region.add(0, 0, width as i32, height as i32);
        surface.set_opaque_region(Some(&region));
        region.destroy();
    }

    /// Changes what keyboard focus one open layer surface asks for, so a
    /// shell can take the keyboard while a page of it is open and give it
    /// back after. The request rides the next frame's commit: a commit of
    /// its own here, with no buffer, made the compositor reconfigure the
    /// surface and stalled the frames the shell was in the middle of.
    /// Hyprland focuses a surface that becomes exclusive and returns focus
    /// when it stops being so. False when the surface is not a layer
    /// surface.
    ///
    /// Without layer-shell the request is recorded instead, and keys typed
    /// into the fallback toplevel go to the latest surface that asked for
    /// them (see `LayerState::key_target`).
    pub fn set_layer_keyboard_focus(&self, id: u64, focus: KeyboardFocus) -> bool {
        let Some(record) = self.state.layers.get(&id) else {
            return false;
        };
        if record.keyboard.get().0 != focus {
            note_keyboard_request(record, focus, self.state.next_layer_sequence());
        }
        let Some(layer) = record.surface.as_layer() else {
            return false;
        };
        layer.set_keyboard_interactivity(layer_interactivity(focus));
        true
    }

    /// Applies the default, empty, or rectangular input region to one surface.
    pub fn set_layer_input_region(&self, id: u64, rectangles: Option<&[InputRect]>) {
        let Some(surface) = self.layer_surface(id) else {
            return;
        };
        let Some(rectangles) = rectangles else {
            surface.set_input_region(None);
            return;
        };
        let qh = self.queue.handle();
        let region = self.state.compositor.wl_compositor().create_region(&qh, ());
        for rectangle in rectangles {
            if rectangle.width > 0 && rectangle.height > 0 {
                region.add(rectangle.x, rectangle.y, rectangle.width, rectangle.height);
            }
        }
        surface.set_input_region(Some(&region));
        region.destroy();
    }

    /// Builds and applies a composable logical input region to one surface.
    pub fn set_layer_composed_input_region(
        &self,
        id: u64,
        regions: &[Region],
    ) -> Result<(), WaylandError> {
        let (width, height) = self
            .layer_logical_size(id)
            .ok_or_else(|| WaylandError("layer surface is not open".into()))?;
        let rectangles = morf_value::region::build(width, height, regions)
            .map_err(|error| WaylandError(error.to_string()))?;
        self.set_layer_input_region(id, Some(&rectangles));
        Ok(())
    }
}

/// Records a keyboard-focus request on a layer surface, stamped with `serial`
/// when it changes, so the latest surface to ask can be found.
pub(super) fn note_keyboard_request(record: &LayerRecord, focus: KeyboardFocus, serial: u64) {
    if record.keyboard.get().0 != focus {
        record.keyboard.set((focus, serial));
    }
}
