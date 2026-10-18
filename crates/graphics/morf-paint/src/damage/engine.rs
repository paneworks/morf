//! The engine a surface paints through: scene to draw list, list to damage,
//! damage to the backend.

use morf_layout::Layout;
use morf_scene::{NodeHandle, Scene};
use std::error::Error as StdError;

use super::explain::damage_log_wanted;
use super::{DamageRect, DamageTracker};
use crate::paint::PaintCache;
use crate::{Copies, DrawList, RenderError};

/// Renderer implementation selected by the surface runtime.
pub trait RenderBackend {
    /// Backend error.
    type Error: StdError + Send + Sync + 'static;

    /// Draws an ordered list, restricting pixel work to damage rectangles.
    fn render(
        &mut self,
        list: &DrawList,
        damage: &[DamageRect],
        scale_120: u32,
    ) -> Result<(), Self::Error>;

    /// Draws a list as [`RenderBackend::render`] does, told which of its
    /// commands were copied unchanged from an earlier one, so a backend that
    /// keeps what it made of that list can keep it for them too.
    fn render_copied(
        &mut self,
        list: &DrawList,
        copies: Copies<'_>,
        damage: &[DamageRect],
        scale_120: u32,
    ) -> Result<(), Self::Error> {
        let _ = copies;
        self.render(list, damage, scale_120)
    }

    /// Resizes the target the frames are painted into.
    ///
    /// Whatever the last frame left there does not survive this, which is why
    /// it is on the trait and not only on the backend: it has to be reachable
    /// through the engine, and the engine is the only thing that knows the
    /// screen has to be repainted in full afterwards.
    fn resize(&mut self, width: u32, height: u32);
}

/// A tree laid out on its own and painted after every root of a frame:
/// over the surface's scene and outside its layout.
#[derive(Clone, Copy, Debug)]
pub struct Overlay<'a> {
    pub root: NodeHandle,
    pub layout: &'a Layout,
    /// Whether it stands for the whole surface, so nothing of the scene
    /// under it is drawn: a picture of the surface while it is rebuilt.
    pub conceals: bool,
}

/// Scene painter and damage tracker driving a selected backend.
pub struct RenderEngine<B> {
    backend: B,
    damage: DamageTracker,
    /// The draw list, kept between frames for its capacity.
    list: DrawList,
    /// Where each subtree sits in the last frame's list, to copy from it.
    paint: PaintCache,
}

impl<B: RenderBackend> RenderEngine<B> {
    /// Wraps a renderer backend with draw-list and damage processing.
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            damage: DamageTracker::default(),
            list: DrawList::default(),
            paint: PaintCache::default(),
        }
    }

    /// The draw list of the last frame painted.
    pub fn last_list(&self) -> &DrawList {
        &self.damage.previous
    }

    /// What the last list made copied from the one before it.
    #[cfg(any(test, feature = "test-support"))]
    pub fn copies(&self) -> Copies<'_> {
        self.paint.copies()
    }

    /// Paints one resolved scene frame.
    pub fn render(
        &mut self,
        scene: &Scene,
        layout: &Layout,
        scale_120: u32,
        declare: impl FnOnce(&[DamageRect]),
    ) -> Result<Vec<DamageRect>, RenderError> {
        self.render_over(scene, layout, &[], scale_120, declare)
    }

    /// Paints one resolved scene frame with `overlays` drawn over it, in
    /// order.
    pub fn render_over(
        &mut self,
        scene: &Scene,
        layout: &Layout,
        overlays: &[Overlay<'_>],
        scale_120: u32,
        declare: impl FnOnce(&[DamageRect]),
    ) -> Result<Vec<DamageRect>, RenderError> {
        // The list is kept between frames so its buffers are not returned to
        // the allocator and reclaimed sixty times a second.
        let mut list = std::mem::take(&mut self.list);
        let built = list.rebuild_from(
            scene,
            layout,
            overlays,
            &self.damage.previous,
            &mut self.paint,
        );
        let result = built.and_then(|()| {
            let copies = self.paint.copies();
            let damage = self.damage.diff_copied(&list, copies.runs, scale_120);
            // The caller sees the damage before anything is presented, because
            // presenting commits the surface and a commit carries whatever
            // damage was declared before it. A host that waits until afterwards
            // has no way to tell the compositor what actually changed, and ends
            // up declaring the whole surface — which on a fullscreen overlay
            // means a full recomposite every frame.
            declare(&damage);
            if !damage.is_empty() {
                self.backend
                    .render_copied(&list, copies, &damage, scale_120)
                    .map_err(|error| RenderError::Backend(error.to_string()))?;
            }
            Ok(damage)
        });
        // Only on success: a failed rebuild leaves a half-built list, and
        // making that the baseline would silently under-damage the next frame.
        if result.is_ok() {
            self.damage.retain(&mut list);
            self.paint.kept();
        } else {
            self.paint.forget();
        }
        self.list = list;
        result
    }

    /// Resizes the target, and forgets what was on it.
    ///
    /// The two belong together. A resize hands back a blank target, so a frame
    /// diffed against the one before it repaints only what changed and leaves
    /// the rest of the screen as the cleared colour — black, with whatever
    /// happens to animate afterwards appearing on it one piece at a time. That
    /// is what this is: the resize goes through the engine so the baseline
    /// cannot be left behind.
    pub fn resize(&mut self, width: u32, height: u32) {
        if damage_log_wanted() {
            eprintln!("damage: whole surface: resized to {width}x{height}");
        }
        self.backend.resize(width, height);
        self.damage.forget();
        self.paint.forget();
    }

    /// Forgets what is on screen, so the next frame is drawn in full.
    ///
    /// For when the backend's target was replaced behind the engine's back —
    /// a change of blend space rebuilds it, and nothing the tracker remembers
    /// is on the new one.
    pub fn forget(&mut self) {
        if damage_log_wanted() {
            eprintln!(
                "damage: whole surface: the target was replaced (blend or subpixel text changed)"
            );
        }
        self.damage.forget();
        self.paint.forget();
    }

    /// Returns the backend for surface-specific operations.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }
}
