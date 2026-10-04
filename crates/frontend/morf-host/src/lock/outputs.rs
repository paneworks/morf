//! The lock's outputs: one lock surface each, and the tree each one draws.
//!
//! A lock configuration gives its trees one of two ways. The older is one
//! root for every output: the file builds a single opaque `Rect`, and each
//! output draws it at its own size, the root resized on the way. That is
//! fine while every output is the same size and nothing reads the size in a
//! binding — but a binding sees one size at a time, so on a laptop with an
//! external monitor everything placed by `screen.width` lands where the first
//! screen would have it.
//!
//! The other is `morf.lock_surface(function(screen) ... end)`: a builder the
//! loop calls once per output, with that output's description and the size of
//! its lock surface. Each output then has a root of its own, and every
//! binding in it closes over its own screen. What the trees share — the
//! password, a PAM conversation, `morf.state` — stays in the one runtime,
//! which is why this is one runtime with a root per output rather than the
//! shell's runtime per output: a lock is one conversation with one person,
//! however many screens show it, and a layout pass per output that re-ran
//! every size binding against each screen in turn would churn, and animate,
//! every node that reads it twice a frame.

use morf_layout::{Layout, Size};
use morf_lua::Runtime;
use morf_render::{RenderEngine, WgpuBackend};
use morf_scene::{Element, NodeHandle};
use morf_app::{LayerClient, Output, WindowId};

use crate::{supervisor::lua_screen, surfaces::*};
use morf_app::Backend as _;

/// One output's lock surface: what draws it, and what it last drew.
#[derive(Default)]
pub struct LockOutput {
    pub renderer: Option<RenderEngine<WgpuBackend>>,
    /// The layout of the last frame, which is what a point on this surface
    /// is hit-tested against.
    pub layout: Option<Layout>,
    /// This output's own tree, when the configuration builds per output.
    pub root: Option<NodeHandle>,
    /// The logical size that tree was built for.
    pub built_for: Option<(u32, u32)>,
}

/// The lock's surfaces, as the input path looks them up.
pub struct LockLayouts<'a>(pub &'a [LockOutput]);

impl SurfaceLayouts for LockLayouts<'_> {
    fn layout_of(&self, surface: WindowId) -> Option<&Layout> {
        match surface {
            WindowId::Lock(index) => self.0.get(index)?.layout.as_ref(),
            _ => None,
        }
    }
}

/// Where the lock's trees come from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockTrees {
    /// The file's one root, drawn on every output.
    Shared(NodeHandle),
    /// A root per output, from `morf.lock_surface`.
    PerOutput,
}

impl LockTrees {
    /// Reads which of the two the configuration chose, and checks it.
    pub fn of(runtime: &Runtime) -> Result<Self, String> {
        let roots = runtime.scene().roots();
        if runtime.has_lock_surface_builder() {
            if !roots.is_empty() {
                return Err(
                    "a lock configuration that builds per output (morf.lock_surface) \
                     must not create a root of its own"
                        .to_owned(),
                );
            }
            return Ok(Self::PerOutput);
        }
        let [root] = roots.as_slice() else {
            return Err("lock configuration must create exactly one root item".to_owned());
        };
        check_lock_root(runtime, *root)?;
        Ok(Self::Shared(*root))
    }

    /// The tree drawn on output `index`.
    pub fn root(self, outputs: &[LockOutput], index: usize) -> Option<NodeHandle> {
        match self {
            Self::Shared(root) => Some(root),
            Self::PerOutput => outputs.get(index)?.root,
        }
    }

    /// The tree a key on `surface` goes into: that surface's, or, for a key
    /// the compositor sent without saying where, the first there is.
    pub fn key_root(
        self,
        outputs: &[LockOutput],
        surface: WindowId,
    ) -> Option<NodeHandle> {
        match surface {
            WindowId::Lock(index) => self.root(outputs, index),
            _ => (0..outputs.len()).find_map(|index| self.root(outputs, index)),
        }
    }
}

/// A lock tree has to hide the session: an opaque rectangle at the top.
pub fn check_lock_root(runtime: &Runtime, root: NodeHandle) -> Result<(), String> {
    if runtime
        .scene()
        .element(root)
        .map_err(|error| error.to_string())?
        != Element::Rect
    {
        return Err("lock configuration root must be an opaque Rect".to_owned());
    }
    Ok(())
}

/// Makes sure output `index` has a tree for a lock surface of `size`, and
/// says whether one was built. A tree built per output is built again when
/// its output changes size, so its bindings see the new one; a shared tree
/// has nothing to build.
pub fn ensure_lock_tree(
    runtime: &mut Runtime,
    trees: LockTrees,
    output: &mut LockOutput,
    index: usize,
    screen: Option<Output>,
    size: (u32, u32),
) -> Result<bool, String> {
    if trees != LockTrees::PerOutput || (output.root.is_some() && output.built_for == Some(size)) {
        return Ok(false);
    }
    release_lock_tree(runtime, output);
    let mut screen = screen.as_ref().map(lua_screen).unwrap_or_default();
    // The size the tree has to cover is the lock surface's, which is the
    // output's logical size by the protocol, but the compositor's word on it
    // is the configure, not the output's advertisement.
    screen.width = i32::try_from(size.0).ok();
    screen.height = i32::try_from(size.1).ok();
    let root = runtime
        .build_lock_surface(&screen, index)
        .map_err(|error| format!("lock surface for output {}: {error}", index + 1))?;
    if let Err(error) = check_lock_root(runtime, root) {
        runtime.remove_lock_surface(root);
        return Err(error);
    }
    output.root = Some(root);
    output.built_for = Some(size);
    output.layout = None;
    Ok(true)
}

/// Takes down the tree an output was drawing, if it had its own.
pub fn release_lock_tree(runtime: &mut Runtime, output: &mut LockOutput) {
    if let Some(root) = output.root.take() {
        runtime.remove_lock_surface(root);
    }
    output.built_for = None;
    output.layout = None;
}

/// A lock root's colour as bytes, for the first frame.
pub fn root_color_bytes(runtime: &Runtime, root: NodeHandle) -> Result<[u8; 4], String> {
    let color = runtime
        .scene()
        .color_value(root, "color")
        .map_err(|error| error.to_string())?;
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    Ok([
        byte(color.red),
        byte(color.green),
        byte(color.blue),
        byte(color.alpha),
    ])
}

/// Sizes a lock root to its surface, writing only what changed: an
/// assignment is a layout invalidation even when it writes the same number.
pub fn size_lock_root(
    runtime: &mut Runtime,
    root: NodeHandle,
    (width, height): (u32, u32),
) -> Result<(), String> {
    let mut scene = runtime.scene_mut();
    for (property, value) in [
        ("x", 0.0),
        ("y", 0.0),
        ("width", f64::from(width)),
        ("height", f64::from(height)),
    ] {
        if scene.number(root, property).ok() != Some(value) {
            scene
                .assign(root, property, value)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub fn paint_lock(
    runtime: &mut Runtime,
    renderer: &mut RenderEngine<WgpuBackend>,
    client: &LayerClient,
    index: usize,
    root: NodeHandle,
) -> Result<Layout, String> {
    let (width, height) = client
        .lock_size(index)
        .ok_or_else(|| "lock surface disappeared while painting".to_owned())?;
    size_lock_root(runtime, root, (width, height))?;
    // The lock screen is `morf.surface` too, so it blends the way that says.
    crate::paint::apply_blend(renderer, &runtime.layer_surface_config().blend);
    let scene = runtime.scene();
    let color = scene
        .color_value(root, "color")
        .map_err(|error| error.to_string())?;
    if color.alpha < 1.0
        || scene
            .number(root, "opacity")
            .map_err(|error| error.to_string())?
            < 1.0
    {
        return Err("lock configuration root must stay opaque".to_owned());
    }
    drop(scene);
    let layout = runtime.compute_layout(
        root,
        Size {
            width: f64::from(width),
            height: f64::from(height),
        },
        renderer.backend_mut(),
    )?;
    runtime.sync_text_inputs(&layout, renderer.backend_mut().text_system());
    runtime.observe_stretch(&layout);
    let scene = runtime.scene();
    client.request_frame(WindowId::Lock(index));
    let scale = client.lock_scale_120(index).unwrap_or(120);
    let damage = renderer
        .render(&scene, &layout, scale, |_| {})
        .map_err(|error| error.to_string())?;
    if damage.is_empty() {
        client.commit(WindowId::Lock(index));
    }
    drop(scene);
    // After the render: what the images became is known once they were drawn.
    runtime.sync_images(&layout, renderer.backend_mut().image_cache());
    runtime.observe_layout(&layout);
    Ok(layout)
}
