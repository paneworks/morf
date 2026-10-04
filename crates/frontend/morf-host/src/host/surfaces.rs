mod sync;

use morf_layout::{Hit, Layout};
use morf_lua::{ToplevelSurfaceConfig, LayerSurfaceConfig, PopupSurfaceConfig, Runtime};
use morf_render::{RenderEngine, WgpuBackend};
use morf_scene::NodeHandle;
use morf_app::{
    LayerConfig, KeyboardFocus, LayerAnchors, LayerClient, Event, PRIMARY_LAYER, ShellLayer,
    WindowId,
};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::host::windows::Windows;
use crate::{pacing::*, paint::*, surface_layers::*};

pub use sync::sync_window_surfaces;

pub struct Window {
    pub id: u64,
    pub root: NodeHandle,
    pub updates_enabled: bool,
    pub width: u32,
    pub height: u32,
    pub renderer: Option<RenderEngine<WgpuBackend>>,
    pub layout: Option<CachedLayout>,
    pub popup_config: Option<PopupSurfaceConfig>,
    pub floating_config: Option<ToplevelSurfaceConfig>,
    pub layer_config: Option<LayerSurfaceConfig>,
    /// Whether this surface has work pending for the next frame callback.
    ///
    /// A configured layer surface is permanent decoration, so repainting it on
    /// every frame callback would keep the compositor compositing forever. It
    /// paints only when something marked it dirty, and asks for another frame
    /// only when it painted.
    pub needs_paint: bool,
    /// A lock surface's own tree: whether `root` is one built for this
    /// output (and so taken down with it), and the size it was built for.
    pub owns_root: bool,
    pub built_for: Option<(u32, u32)>,
}

/// Largest frame delta charged to animations in a single tick.
///
/// A compositor that fell behind should let motion catch up, but only so far.
/// Beyond a few dropped frames, advancing by the whole gap reads as a jump, so
/// the tick is capped and the remaining time is simply lost.
pub const MAX_FRAME_DELTA_MS: u32 = 100;

/// How far to advance animations for a frame callback at `time_ms`.
///
/// `previous` is the timebase carried forward from the last tick, and is absent
/// whenever the scene had settled. That absence is what keeps idle time out of
/// the clock: the compositor stops sending frame callbacks while nothing moves,
/// so the gap since the last one measures how long the shell sat still, not how
/// far motion should advance. Charging it to an animation that started in the
/// meantime makes it jump, and a long enough gap lands it on its target in a
/// single tick.
pub fn animation_delta(previous: Option<u32>, time_ms: u32) -> Duration {
    let elapsed = previous.map_or(0, |previous| {
        time_ms.wrapping_sub(previous).min(MAX_FRAME_DELTA_MS)
    });
    Duration::from_millis(elapsed.into())
}

/// What the pointer and the fingers are doing, across every surface one
/// client owns.
///
/// Its own struct, apart from the surfaces, because two loops keep one: the
/// shell's, over layer surfaces, popups and floating windows, and the lock's,
/// over one lock surface per output. Both route input through the same code
/// and differ only in how a surface's layout is found.
pub struct PointerInput {
    /// Where the pointer is: the surface it is over and its point there,
    /// until it leaves. What `contains_pointer` is worked out against.
    pub pointer: Option<(WindowId, f64, f64)>,
    pub hovered: Option<(WindowId, Hit)>,
    pub pressed: Option<(WindowId, Hit, f64, f64, bool)>,
    /// The button behind `pressed`, so its release and click say which.
    pub pressed_button: u32,
    pub focused: HashMap<WindowId, NodeHandle>,
    /// Each finger down: where it landed, where it was last, and how far
    /// it has travelled, which is what tells a tap from a swipe.
    pub touches: HashMap<i32, (WindowId, Hit, f64, f64, f64)>,
    /// Pinches and edge swipes the touches are making.
    pub gestures: crate::surface_gesture::TouchGestures,
}

impl Default for PointerInput {
    fn default() -> Self {
        Self {
            pointer: None,
            hovered: None,
            pressed: None,
            pressed_button: 0x110,
            focused: HashMap::new(),
            touches: HashMap::new(),
            gestures: Default::default(),
        }
    }
}

impl PointerInput {
    /// Forgets everything: the nodes it names may no longer exist.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// How the input path finds the layout a surface was last drawn with, which
/// is what a point on that surface is hit-tested against.
pub trait SurfaceLayouts {
    fn layout_of(&self, surface: WindowId) -> Option<&Layout>;
}

/// The shell's surfaces: the primary layer and whatever hangs off it.
pub struct LayerLayouts<'a> {
    pub layout: &'a Layout,
    pub windows: &'a Windows,
}

impl SurfaceLayouts for LayerLayouts<'_> {
    fn layout_of(&self, surface: WindowId) -> Option<&Layout> {
        surface_layout(surface, self.layout, self.windows)
    }
}

pub struct SurfaceEventState {
    pub layout: CachedLayout,
    /// The scene root this output's main surface draws.
    ///
    /// Resolved once and again whenever the window-surface set changes, which
    /// is the only thing that can move it. Working it out afresh walked every
    /// node in the scene, deep-cloned every window-surface config and allocated
    /// three collections — on every repaint and every key press.
    pub primary_root: NodeHandle,
    /// Every popup, toplevel and extra layer surface that is live.
    pub windows: Windows,
    pub last_frame: Option<u32>,
    /// What this surface can afford, and when it last painted.
    pub pacer: FramePacer,
    /// Whether any registered shader reads the clock.
    ///
    /// Such a shader has to be redrawn continuously, but *through* the frame
    /// callback like everything else — treating it as a reason to repaint
    /// outside the callback is a spin, not an animation.
    pub animating_shaders: bool,
    /// The interval between the compositor's frame callbacks, as measured.
    pub refresh: Duration,
    /// Where the pointer, the buttons and the fingers are.
    pub input: PointerInput,
    /// A drag from another application over one of these surfaces.
    pub drag: Option<crate::surface_drag::DragFollow>,
    /// Whether the shell's own surface owes a paint it could not make
    /// because its last frame callback had not come back yet; the callback
    /// makes it.
    pub primary_deferred: bool,
    /// Surfaces that gained or lost the keyboard since the loop last told
    /// the screen reader (`surface_a11y.rs`).
    pub keyboard_changes: Vec<(NodeHandle, bool)>,
    /// When the wall clock last advanced motion, while the shell's own
    /// surface gets no frame callbacks (hidden under another toplevel in a
    /// nested compositor). `None` while callbacks drive it as usual.
    pub fallback_tick: Option<std::time::Instant>,
    /// When a paint the shell owed was last made without waiting any longer
    /// for an overdue frame callback ([`owed_paint_due`]).
    pub forced_paint: Option<std::time::Instant>,
}

/// Whether a paint the shell's own surface owes -- deferred until its frame
/// callback, which is now overdue -- is made anyway, at most once a stall.
///
/// A compositor answers a frame callback when it next draws the output, and
/// one that draws only what is damaged (wlroots under a headless or idle
/// output: cage) may not draw again after an empty commit. A change that
/// moves nothing -- a panel shown, a list filled from a timer, an IPC call --
/// then waited for whatever next made the compositor draw: the pointer, a
/// key, a screenshot. What the shell showed was the state from the change
/// before. Motion already had the wall clock to fall back on; this is the
/// same for a single paint.
pub fn owed_paint_due(
    deferred: bool,
    waiting: Option<std::time::Duration>,
    refresh: std::time::Duration,
    forced: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    let stall = frame_stall(refresh);
    deferred
        && waiting.is_some_and(|waiting| waiting > stall)
        && forced.is_none_or(|at| now.saturating_duration_since(at) > stall)
}

/// How long the shell's own surface may wait for a frame callback before
/// the wall clock takes over advancing motion: a few refreshes, so a slow
/// frame is not mistaken for a hidden surface.
pub fn frame_stall(refresh: Duration) -> Duration {
    (refresh * 4).clamp(Duration::from_millis(50), Duration::from_millis(250))
}

pub fn surface_layout<'a>(
    surface: WindowId,
    layer: &'a Layout,
    windows: &'a Windows,
) -> Option<&'a Layout> {
    match surface {
        WindowId::Layer(PRIMARY_LAYER) => Some(layer),
        _ => Some(&windows.by_window(surface)?.layout.as_ref()?.layout),
    }
}

pub fn surface_root(surface: WindowId, layer: NodeHandle, windows: &Windows) -> Option<NodeHandle> {
    match surface {
        WindowId::Layer(PRIMARY_LAYER) => Some(layer),
        _ => windows.by_window(surface).map(|surface| surface.root),
    }
}

pub fn primary_surface_root(runtime: &Runtime) -> Result<NodeHandle, String> {
    let roots = runtime.scene().roots();
    let mut window_roots = HashSet::new();
    for surface in runtime.window_surface_configs() {
        if !window_roots.insert(surface.root) {
            return Err("a scene root cannot back multiple window surfaces".into());
        }
        if runtime
            .scene()
            .parent(surface.root)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err("window surface roots must be top-level scene nodes".into());
        }
    }
    let primary = roots
        .into_iter()
        .filter(|root| !window_roots.contains(root))
        .collect::<Vec<_>>();
    match primary.as_slice() {
        [] => Err("configuration must create exactly one primary surface root".into()),
        [only] => Ok(*only),
        many => {
            // A node left at the top level -- built and never parented, or
            // unparented and never destroyed -- is a bug in the
            // configuration, not a reason to take the shell down. The shell's
            // own root is the one whose tree is by far the largest; the rest
            // are named once so they can be found.
            let scene = runtime.scene();
            let chosen = *many
                .iter()
                .max_by_key(|root| subtree_size(&scene, **root))
                .expect("many is not empty");
            for stray in many.iter().filter(|root| **root != chosen) {
                warn_stray_root(&scene, *stray);
            }
            Ok(chosen)
        }
    }
}

/// [`primary_surface_root`], keeping `previous` while it is still a root that
/// no window surface has taken: a stray node never displaces the shell.
pub fn primary_surface_root_keeping(
    runtime: &Runtime,
    previous: NodeHandle,
) -> Result<NodeHandle, String> {
    let still_primary = runtime
        .scene()
        .parent(previous)
        .is_ok_and(|parent| parent.is_none())
        && !runtime
            .window_surface_configs()
            .iter()
            .any(|surface| surface.root == previous);
    if still_primary {
        // Still name any strays, once.
        let _ = primary_surface_root(runtime);
        return Ok(previous);
    }
    primary_surface_root(runtime)
}

fn subtree_size(scene: &morf_scene::Scene, root: NodeHandle) -> usize {
    let mut count = 0;
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        count += 1;
        if let Ok(children) = scene.children(node) {
            pending.extend(children.iter().copied());
        }
    }
    count
}

fn warn_stray_root(scene: &morf_scene::Scene, root: NodeHandle) {
    thread_local! {
        static WARNED: std::cell::RefCell<HashSet<NodeHandle>> = Default::default();
    }
    if !WARNED.with(|warned| warned.borrow_mut().insert(root)) {
        return;
    }
    let element = scene
        .element(root)
        .map(|element| format!("{element:?}"))
        .unwrap_or_default();
    let id = match scene.string_value(root, "id") {
        Ok(id) if !id.is_empty() => format!(" id={id:?}"),
        _ => String::new(),
    };
    let children = scene
        .children(root)
        .map(|children| children.len())
        .unwrap_or(0);
    eprintln!(
        "morf: a {element}{id} with {children} children sits at the top level with no \
         surface; it is not drawn (build it inside a surface, or ui.destroy it)"
    );
}

/// The zone a surface reserves when it asks for "auto".
///
/// Its own extent on the edge it is anchored to, plus the margin between it
/// and that edge, because the margin is space the surface has claimed too. A
/// surface anchored to opposite edges spans the output and reserves nothing:
/// there is no side to push windows towards.
pub fn auto_exclusive_zone(surface: &LayerSurfaceConfig) -> i32 {
    let anchors = &surface.anchors;
    let height = i32::try_from(surface.height).unwrap_or(i32::MAX);
    let width = i32::try_from(surface.width).unwrap_or(i32::MAX);
    match (anchors.top, anchors.bottom, anchors.left, anchors.right) {
        (true, false, _, _) => height.saturating_add(surface.margin_top),
        (false, true, _, _) => height.saturating_add(surface.margin_bottom),
        (_, _, true, false) => width.saturating_add(surface.margin_left),
        (_, _, false, true) => width.saturating_add(surface.margin_right),
        _ => 0,
    }
}

/// The focus policy a configuration's word names.
pub fn keyboard_focus_of(value: &str) -> Option<KeyboardFocus> {
    match value {
        "none" => Some(KeyboardFocus::None),
        "exclusive" => Some(KeyboardFocus::Exclusive),
        "on_demand" => Some(KeyboardFocus::OnDemand),
        _ => None,
    }
}

pub fn runtime_bar_config(
    surface: &LayerSurfaceConfig,
    output: &str,
) -> Result<LayerConfig, String> {
    let layer = match surface.layer.as_str() {
        "background" => ShellLayer::Background,
        "bottom" => ShellLayer::Bottom,
        "top" => ShellLayer::Top,
        "overlay" => ShellLayer::Overlay,
        value => return Err(format!("unsupported layer surface layer `{value}`")),
    };
    let keyboard_focus = keyboard_focus_of(&surface.keyboard_focus).ok_or_else(|| {
        format!(
            "unsupported keyboard focus policy `{}`",
            surface.keyboard_focus
        )
    })?;
    Ok(LayerConfig {
        namespace: surface.namespace.clone(),
        width: surface.width,
        height: surface.height,
        exclusive_zone: if surface.exclusive_auto {
            auto_exclusive_zone(surface)
        } else {
            surface.exclusive_zone
        },
        output: Some(output.to_owned()),
        anchors: LayerAnchors {
            top: surface.anchors.top,
            right: surface.anchors.right,
            bottom: surface.anchors.bottom,
            left: surface.anchors.left,
        },
        margin_top: surface.margin_top,
        margin_right: surface.margin_right,
        margin_bottom: surface.margin_bottom,
        margin_left: surface.margin_left,
        layer,
        keyboard_focus,
    })
}

pub fn connect_runtime_surface(
    runtime: &Runtime,
    output: &str,
) -> Result<LayerClient, String> {
    let config = runtime.layer_surface_config();
    let mut client =
        LayerClient::connect(runtime_bar_config(&config, output)?).map_err(|e| e.to_string())?;
    open_reserve_layers(&mut client, &config, output)?;
    crate::backdrop::open_backdrop_layer(&mut client, &config, output)?;
    loop {
        client.blocking_dispatch().map_err(|error| error.to_string())?;
        while let Some(event) = client.next_event() {
            match event {
                Event::Configure { id, .. } if id == PRIMARY_LAYER => return Ok(client),
                Event::Closed { id } if id == PRIMARY_LAYER => {
                    return Err(crate::supervisor::SURFACE_CLOSED.to_owned());
                }
                _ => {}
            }
        }
    }
}
