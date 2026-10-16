//! Painting the auxiliary surfaces: popups and floating windows.

use morf_app::{Backend, WindowId, physical_size};
use morf_lua::Runtime;
use morf_text::TextSystem;

use crate::host::surfaces::Window;

use super::cache::{CachedLayout, layout_for};
use super::layout_only::lay_out_auxiliary;
use super::renderer_settings::{apply_blend, apply_subpixel};

/// Which kind of auxiliary surface a paint is for.
///
/// The popup and the floating window are painted by identical code — they were
/// two copies of the same fifty lines, differing in four identifiers, which is
/// two places for every future fix to have to be applied. The only thing that
/// actually differs is which of the client's four accessors to reach for.
#[derive(Clone, Copy)]
pub enum AuxiliaryKind {
    Popup,
    Floating,
}

impl AuxiliaryKind {
    /// How this surface is addressed, which is what its own scale is keyed on.
    pub fn role(self, id: u64) -> WindowId {
        match self {
            Self::Popup => WindowId::Popup(id),
            Self::Floating => WindowId::Toplevel(id),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Popup => "popup",
            Self::Floating => "toplevel",
        }
    }

    pub fn request_frame(self, client: &dyn Backend, id: u64) {
        match self {
            Self::Popup => client.request_frame(WindowId::Popup(id)),
            Self::Floating => client.request_frame(WindowId::Toplevel(id)),
        }
    }

    /// Declares the whole surface damaged, ahead of the render that fills it.
    pub fn damage(
        self,
        client: &dyn Backend,
        id: u64,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let window = self.window(id);
        if !client.has_window(window) {
            return Err(format!(
                "{} surface disappeared while painting",
                self.name()
            ));
        }
        client.damage(window, 0, 0, width as i32, height as i32);
        Ok(())
    }

    pub fn commit(self, client: &dyn Backend, id: u64) {
        client.commit(self.window(id));
    }

    fn window(self, id: u64) -> WindowId {
        match self {
            Self::Popup => WindowId::Popup(id),
            Self::Floating => WindowId::Toplevel(id),
        }
    }
}

/// `text`: the host's text system when it draws nothing (see
/// [`crate::paint::painter::Painter::layout_only`]); the window is laid out
/// with it instead.
pub fn paint_popup_surface(
    runtime: &mut Runtime,
    client: &dyn Backend,
    surface: &mut Window,
    text: Option<&mut TextSystem>,
) -> Result<(), String> {
    let _stage = morf_lua::profile::stage(morf_lua::profile::Stage::Paint);
    match text {
        Some(text) => lay_out_auxiliary(AuxiliaryKind::Popup, runtime, text, client, surface),
        None => paint_auxiliary_surface(AuxiliaryKind::Popup, runtime, client, surface),
    }
}

pub fn paint_floating_surface(
    runtime: &mut Runtime,
    client: &dyn Backend,
    surface: &mut Window,
    text: Option<&mut TextSystem>,
) -> Result<(), String> {
    let _stage = morf_lua::profile::stage(morf_lua::profile::Stage::Paint);
    match text {
        Some(text) => lay_out_auxiliary(AuxiliaryKind::Floating, runtime, text, client, surface),
        None => paint_auxiliary_surface(AuxiliaryKind::Floating, runtime, client, surface),
    }
}

/// Paints one popup or floating surface.
pub fn paint_auxiliary_surface(
    kind: AuxiliaryKind,
    runtime: &mut Runtime,
    client: &dyn Backend,
    surface: &mut Window,
) -> Result<(), String> {
    let Some(renderer) = &mut surface.renderer else {
        return Ok(());
    };
    let what = format!("{} {}", kind.name(), surface.id);
    crate::paint::recover::recover_lost_device(runtime, renderer, &what)?;
    let blend = match kind {
        AuxiliaryKind::Popup => surface.popup_config.as_ref().map(|config| &config.blend),
        AuxiliaryKind::Floating => surface.floating_config.as_ref().map(|config| &config.blend),
    };
    if let Some(blend) = blend {
        apply_blend(renderer, blend);
    }
    apply_subpixel(
        renderer,
        &runtime.layer_surface_config().subpixel_text,
        client,
        false,
    );
    let revision = runtime.scene().layout_revision_of(surface.root);
    let size = (surface.width, surface.height);
    // This surface's own scale, not the bar's. A popup opened from a panel on a
    // 1x screen but shown on a 2x one was drawn at 1x and stretched -- and on a
    // mixed-DPI desk that is most popups.
    let scale_120 = client.surface_scale_120(kind.role(surface.id));
    let (layout, fresh) = layout_for(
        runtime,
        surface.layout.as_mut(),
        surface.root,
        (revision, size, scale_120),
        renderer.backend_mut(),
    )?;
    runtime.sync_text_inputs(&layout, renderer.backend_mut().text_system());
    runtime.observe_stretch(&layout);
    let scene = runtime.scene();
    let (width, height) = client
        .buffer_size(kind.role(surface.id))
        .unwrap_or_else(|| physical_size((surface.width, surface.height), scale_120));
    if !renderer.backend_mut().declares_damage() {
        kind.damage(client, surface.id, width, height)?;
    }
    let damage = match renderer.render(&scene, &layout, scale_120, |_| {}) {
        Err(_) if crate::paint::recover::recover_lost_device(runtime, renderer, &what)? => {
            renderer.render(&scene, &layout, scale_120, |_| {})
        }
        rendered => rendered,
    }
    .map_err(|error| error.to_string())?;
    // Only ask for another callback when this paint actually drew something.
    //
    // Asking unconditionally is a loop with no exit: the callback repaints, the
    // repaint asks for a callback, and a popup that has been sitting still for
    // an hour still costs a full paint every refresh. Anything that changes the
    // scene repaints these surfaces through the main loop anyway, so the
    // callback is a throttle for motion, not the thing that keeps them alive.
    if damage.is_empty() {
        kind.commit(client, surface.id);
    } else {
        kind.request_frame(client, surface.id);
    }
    drop(scene);
    // After the render: what the images became is known once they were drawn.
    runtime.sync_images(&layout, renderer.backend_mut().image_cache());
    runtime.observe_layout_with(&layout, fresh);
    // A binding on the layout moved this tree as the frame was observed (see
    // `paint_layer_surface`): the frame callback repaints it, asked for here
    // when the render did not.
    if damage.is_empty() && runtime.scene().layout_revision_of(surface.root) != revision {
        kind.request_frame(client, surface.id);
    }
    surface.layout = Some(CachedLayout {
        painted: None,
        layout,
        revision,
        size,
        scale_120,
        input: Vec::new(),
        input_paint: u64::MAX,
        backdrop: Vec::new(),
        keyboard_focus: String::new(),
    });
    Ok(())
}
