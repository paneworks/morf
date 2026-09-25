use morf_layout::{Hit, Layout};
use morf_lua::{
    FloatingSurfaceConfig, LayerSurfaceConfig, PopupSurfaceConfig, Runtime, WindowSurfaceKind,
};
use morf_render::{RenderEngine, WgpuBackend};
use morf_scene::NodeHandle;
use morf_wayland::{
    BarConfig, FloatingConfig, KeyboardFocus, LayerAnchors, LayerClient, LayerEvent, PRIMARY_LAYER,
    ShellLayer, SurfaceRole,
};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::{pacing::*, paint::*, surface_layers::*, surface_popups::*};

pub(crate) struct AuxiliarySurface {
    pub(crate) id: u64,
    pub(crate) root: NodeHandle,
    pub(crate) updates_enabled: bool,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) renderer: Option<RenderEngine<WgpuBackend>>,
    pub(crate) layout: Option<CachedLayout>,
    pub(crate) popup_config: Option<PopupSurfaceConfig>,
    pub(crate) floating_config: Option<FloatingSurfaceConfig>,
    pub(crate) layer_config: Option<LayerSurfaceConfig>,
    /// Whether this surface has work pending for the next frame callback.
    ///
    /// A configured layer surface is permanent decoration, so repainting it on
    /// every frame callback would keep the compositor compositing forever. It
    /// paints only when something marked it dirty, and asks for another frame
    /// only when it painted.
    pub(crate) needs_paint: bool,
}

/// Largest frame delta charged to animations in a single tick.
///
/// A compositor that fell behind should let motion catch up, but only so far.
/// Beyond a few dropped frames, advancing by the whole gap reads as a jump, so
/// the tick is capped and the remaining time is simply lost.
pub(crate) const MAX_FRAME_DELTA_MS: u32 = 100;

/// How far to advance animations for a frame callback at `time_ms`.
///
/// `previous` is the timebase carried forward from the last tick, and is absent
/// whenever the scene had settled. That absence is what keeps idle time out of
/// the clock: the compositor stops sending frame callbacks while nothing moves,
/// so the gap since the last one measures how long the shell sat still, not how
/// far motion should advance. Charging it to an animation that started in the
/// meantime makes it jump, and a long enough gap lands it on its target in a
/// single tick.
pub(crate) fn animation_delta(previous: Option<u32>, time_ms: u32) -> Duration {
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
pub(crate) struct PointerInput {
    pub(crate) hovered: Option<(SurfaceRole, Hit)>,
    pub(crate) pressed: Option<(SurfaceRole, Hit, f64, f64, bool)>,
    /// The button behind `pressed`, so its release and click say which.
    pub(crate) pressed_button: u32,
    pub(crate) focused: HashMap<SurfaceRole, NodeHandle>,
    /// Each finger down: where it landed, where it was last, and how far
    /// it has travelled, which is what tells a tap from a swipe.
    pub(crate) touches: HashMap<i32, (SurfaceRole, Hit, f64, f64, f64)>,
}

impl Default for PointerInput {
    fn default() -> Self {
        Self {
            hovered: None,
            pressed: None,
            pressed_button: 0x110,
            focused: HashMap::new(),
            touches: HashMap::new(),
        }
    }
}

impl PointerInput {
    /// Forgets everything: the nodes it names may no longer exist.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// How the input path finds the layout a surface was last drawn with, which
/// is what a point on that surface is hit-tested against.
pub(crate) trait SurfaceLayouts {
    fn layout_of(&self, surface: SurfaceRole) -> Option<&Layout>;
}

/// The shell's surfaces: the primary layer and whatever hangs off it.
pub(crate) struct LayerLayouts<'a> {
    pub(crate) layout: &'a Layout,
    pub(crate) popups: &'a HashMap<u64, AuxiliarySurface>,
    pub(crate) floatings: &'a HashMap<u64, AuxiliarySurface>,
    pub(crate) layers: &'a HashMap<u64, AuxiliarySurface>,
}

impl SurfaceLayouts for LayerLayouts<'_> {
    fn layout_of(&self, surface: SurfaceRole) -> Option<&Layout> {
        surface_layout(
            surface,
            self.layout,
            self.popups,
            self.floatings,
            self.layers,
        )
    }
}

pub(crate) struct SurfaceEventState {
    pub(crate) layout: CachedLayout,
    /// The scene root this output's main surface draws.
    ///
    /// Resolved once and again whenever the window-surface set changes, which
    /// is the only thing that can move it. Working it out afresh walked every
    /// node in the scene, deep-cloned every window-surface config and allocated
    /// three collections — on every repaint and every key press.
    pub(crate) primary_root: NodeHandle,
    pub(crate) popup_surfaces: HashMap<u64, AuxiliarySurface>,
    pub(crate) floating_surfaces: HashMap<u64, AuxiliarySurface>,
    pub(crate) layer_surfaces: HashMap<u64, AuxiliarySurface>,
    pub(crate) last_frame: Option<u32>,
    /// What this surface can afford, and when it last painted.
    pub(crate) pacer: FramePacer,
    /// Whether any registered shader reads the clock.
    ///
    /// Such a shader has to be redrawn continuously, but *through* the frame
    /// callback like everything else — treating it as a reason to repaint
    /// outside the callback is a spin, not an animation.
    pub(crate) animating_shaders: bool,
    /// The interval between the compositor's frame callbacks, as measured.
    pub(crate) refresh: Duration,
    /// Where the pointer, the buttons and the fingers are.
    pub(crate) input: PointerInput,
    /// A drag from another application over one of these surfaces.
    pub(crate) drag: Option<crate::surface_drag::DragFollow>,
    /// Whether the shell's own surface owes a paint it could not make
    /// because its last frame callback had not come back yet; the callback
    /// makes it.
    pub(crate) primary_deferred: bool,
    /// When the wall clock last advanced motion, while the shell's own
    /// surface gets no frame callbacks (hidden under another toplevel in a
    /// nested compositor). `None` while callbacks drive it as usual.
    pub(crate) fallback_tick: Option<std::time::Instant>,
    /// When a paint the shell owed was last made without waiting any longer
    /// for an overdue frame callback ([`owed_paint_due`]).
    pub(crate) forced_paint: Option<std::time::Instant>,
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
pub(crate) fn owed_paint_due(
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
pub(crate) fn frame_stall(refresh: Duration) -> Duration {
    (refresh * 4).clamp(Duration::from_millis(50), Duration::from_millis(250))
}

/// Opens, moves and closes the popup, floating and layer windows the
/// configuration asks for. A popup or floating window taken off screen here
/// has its `on_closed` run once the sync is done.
pub(crate) fn sync_window_surfaces(
    runtime: &mut Runtime,
    client: &mut LayerClient,
    popups: &mut HashMap<u64, AuxiliarySurface>,
    floatings: &mut HashMap<u64, AuxiliarySurface>,
    layers: &mut HashMap<u64, AuxiliarySurface>,
    output: &str,
) -> Result<bool, String> {
    let mut resumed = false;
    let surfaces = runtime.window_surface_configs();
    let surfaces_by_id = surfaces
        .iter()
        .map(|surface| (surface.id, surface))
        .collect::<HashMap<_, _>>();
    // A surface is wanted only when it and every ancestor it hangs off are
    // visible, and the three kinds are then handled in identifier order so a
    // parent is always opened before the child anchored to it.
    let desired = |wanted: fn(&WindowSurfaceKind) -> bool| {
        let mut surfaces = surfaces
            .iter()
            .filter(|surface| {
                wanted(&surface.kind)
                    && window_surface_effectively_visible(
                        surface.id,
                        &surfaces_by_id,
                        &mut HashSet::new(),
                    )
            })
            .collect::<Vec<_>>();
        surfaces.sort_by_key(|surface| surface.id);
        surfaces
    };
    let desired_popups = desired(|kind| matches!(kind, WindowSurfaceKind::Popup(_)));
    let desired_floatings = desired(|kind| matches!(kind, WindowSurfaceKind::Floating(_)));
    let desired_layers = desired(|kind| matches!(kind, WindowSurfaceKind::Layer(_)));
    let desired_popup_ids = desired_popups
        .iter()
        .map(|surface| surface.id)
        .collect::<HashSet<_>>();
    let desired_floating_ids = desired_floatings
        .iter()
        .map(|surface| surface.id)
        .collect::<HashSet<_>>();

    let mut stale_popups = popups
        .keys()
        .filter(|id| !desired_popup_ids.contains(id))
        .copied()
        .collect::<Vec<_>>();
    stale_popups.sort_unstable_by(|a, b| b.cmp(a));
    let mut closed = Vec::new();
    for id in stale_popups {
        client.close_popup(id);
        popups.remove(&id);
        closed.push(id);
    }
    let mut stale_floatings = floatings
        .keys()
        .filter(|id| !desired_floating_ids.contains(id))
        .copied()
        .collect::<Vec<_>>();
    stale_floatings.sort_unstable_by(|a, b| b.cmp(a));
    for id in stale_floatings {
        client.close_floating(id);
        floatings.remove(&id);
        closed.push(id);
    }
    resumed |= sync_layer_surfaces(client, output, &desired_layers, layers)?;
    let mut reopened = HashSet::new();
    for surface in desired_floatings {
        let id = surface.id;
        let WindowSurfaceKind::Floating(config) = &surface.kind else {
            unreachable!();
        };
        let changed = floatings
            .get(&id)
            .is_none_or(|current| current.floating_config.as_ref() != Some(config))
            || config
                .parent
                .is_some_and(|parent| reopened.contains(&parent));
        if changed {
            client.close_floating(id);
            client
                .open_floating(
                    id,
                    config.parent,
                    FloatingConfig {
                        width: config.width,
                        height: config.height,
                        minimum_width: config.minimum_width,
                        minimum_height: config.minimum_height,
                        maximum_width: config.maximum_width,
                        maximum_height: config.maximum_height,
                        title: config.title.clone(),
                        app_id: config.app_id.clone(),
                        minimized: config.minimized,
                        maximized: config.maximized,
                        fullscreen: config.fullscreen,
                    },
                )
                .map_err(|error| error.to_string())?;
            reopened.insert(id);
            floatings.insert(
                id,
                AuxiliarySurface {
                    id: surface.id,
                    root: surface.root,
                    updates_enabled: surface.updates_enabled,
                    width: config.width,
                    height: config.height,
                    renderer: None,
                    layout: None,
                    popup_config: None,
                    floating_config: Some(config.clone()),
                    layer_config: None,
                    needs_paint: true,
                },
            );
        } else if let Some(current) = floatings.get_mut(&id) {
            // The stored size is the compositor's, from its last configure,
            // and is left alone: a change to the requested size is a change
            // to the config and reopens the window above. Writing the
            // requested size here put a window the person had resized back
            // to its first size on every sync — until the next configure.
            resumed |= !current.updates_enabled && surface.updates_enabled;
            let moved = current.root != surface.root;
            current.root = surface.root;
            current.updates_enabled = surface.updates_enabled;
            // Only when the tree it lays out actually changed. `CachedLayout`
            // already re-checks the revision, the size and the scale, so
            // clearing it here on every sync threw away a valid layout — and
            // with an anchored popup, which re-syncs whenever its anchor moves,
            // that was every frame.
            if moved {
                current.layout = None;
            }
        }
    }
    for surface in desired_popups {
        let id = surface.id;
        let WindowSurfaceKind::Popup(config) = &surface.kind else {
            unreachable!();
        };
        // A popup the compositor has dismissed is gone from the client while the
        // host still tracks it, and has nothing left to reposition.
        let tracked = popups
            .get(&id)
            .and_then(|current| current.popup_config.as_ref())
            .filter(|_| client.popup_surface(id).is_some());
        // A popup whose parent was just re-created is anchored to a surface that
        // no longer exists, so it follows its parent down whatever its geometry.
        let mut structural = tracked
            .is_none_or(|tracked| popup_change_is_structural(tracked, config))
            || config
                .parent
                .is_some_and(|parent| reopened.contains(&parent));
        if !structural && tracked != Some(config) {
            // Only the positioner moved, so the popup moves with its wl_surface,
            // its GPU surface and its swapchain all intact. A compositor whose
            // `xdg_popup` predates version 3 has no `reposition` request and says
            // so by changing nothing; then the popup has to be rebuilt after all.
            structural = !client
                .reposition_popup(id, popup_client_config(config)?)
                .map_err(|error| error.to_string())?;
        }
        if structural {
            let parent = popup_parent_role(config, &surfaces_by_id)?;
            open_popup_surface(client, surface, config, parent, popups)?;
            reopened.insert(id);
        } else if let Some(current) = popups.get_mut(&id) {
            // The stored size is deliberately left alone. A repositioned popup
            // keeps its current dimensions until the compositor answers with the
            // configure carrying the geometry it settled on, and that configure
            // is also what resizes the swapchain — writing the requested size
            // here would let the two disagree for a frame.
            resumed |= !current.updates_enabled && surface.updates_enabled;
            let moved = current.root != surface.root;
            current.root = surface.root;
            current.updates_enabled = surface.updates_enabled;
            current.popup_config = Some(config.clone());
            if moved {
                current.layout = None;
            }
        }
    }
    // Last, with every surface settled: a callback that opens another window
    // is heard by the next sync rather than this one.
    for id in closed {
        resumed |= runtime.dispatch_window_closed(id);
    }
    Ok(resumed)
}

pub(crate) fn surface_layout<'a>(
    surface: SurfaceRole,
    layer: &'a Layout,
    popups: &'a HashMap<u64, AuxiliarySurface>,
    floatings: &'a HashMap<u64, AuxiliarySurface>,
    layers: &'a HashMap<u64, AuxiliarySurface>,
) -> Option<&'a Layout> {
    match surface {
        SurfaceRole::Layer(PRIMARY_LAYER) => Some(layer),
        SurfaceRole::Layer(id) => {
            Some(&layers.get(&window_surface_id(id)?)?.layout.as_ref()?.layout)
        }
        SurfaceRole::Popup(id) => Some(&popups.get(&id)?.layout.as_ref()?.layout),
        SurfaceRole::Floating(id) => Some(&floatings.get(&id)?.layout.as_ref()?.layout),
        SurfaceRole::Lock(_) => None,
    }
}

pub(crate) fn surface_root(
    surface: SurfaceRole,
    layer: NodeHandle,
    popups: &HashMap<u64, AuxiliarySurface>,
    floatings: &HashMap<u64, AuxiliarySurface>,
    layers: &HashMap<u64, AuxiliarySurface>,
) -> Option<NodeHandle> {
    match surface {
        SurfaceRole::Layer(PRIMARY_LAYER) => Some(layer),
        SurfaceRole::Layer(id) => layers
            .get(&window_surface_id(id)?)
            .map(|surface| surface.root),
        SurfaceRole::Popup(id) => popups.get(&id).map(|surface| surface.root),
        SurfaceRole::Floating(id) => floatings.get(&id).map(|surface| surface.root),
        SurfaceRole::Lock(_) => None,
    }
}

pub(crate) fn primary_surface_root(runtime: &Runtime) -> Result<NodeHandle, String> {
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
pub(crate) fn primary_surface_root_keeping(
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
pub(crate) fn auto_exclusive_zone(surface: &LayerSurfaceConfig) -> i32 {
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
pub(crate) fn keyboard_focus_of(value: &str) -> Option<KeyboardFocus> {
    match value {
        "none" => Some(KeyboardFocus::None),
        "exclusive" => Some(KeyboardFocus::Exclusive),
        "on_demand" => Some(KeyboardFocus::OnDemand),
        _ => None,
    }
}

pub(crate) fn runtime_bar_config(
    surface: &LayerSurfaceConfig,
    output: &str,
) -> Result<BarConfig, String> {
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
    Ok(BarConfig {
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

pub(crate) fn connect_runtime_surface(
    runtime: &Runtime,
    output: &str,
) -> Result<LayerClient, String> {
    let config = runtime.layer_surface_config();
    let mut client =
        LayerClient::connect(runtime_bar_config(&config, output)?).map_err(|e| e.to_string())?;
    open_reserve_layers(&mut client, &config, output)?;
    crate::backdrop::open_backdrop_layer(&mut client, &config, output)?;
    loop {
        client.dispatch().map_err(|error| error.to_string())?;
        while let Some(event) = client.next_event() {
            match event {
                LayerEvent::Configure { id, .. } if id == PRIMARY_LAYER => return Ok(client),
                LayerEvent::Closed { id } if id == PRIMARY_LAYER => {
                    return Err(crate::supervisor::SURFACE_CLOSED.to_owned());
                }
                _ => {}
            }
        }
    }
}
