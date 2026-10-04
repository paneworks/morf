mod layer;

use morf_app::Backend;
use morf_app::{InputRect, PRIMARY_LAYER, WindowId, physical_size};
use morf_layout::{Layout, Size};
use morf_lua::Runtime;
use morf_render::{BlendSpace, RenderEngine, WgpuBackend};
use morf_scene::NodeHandle;
use morf_value::region::Region;

use crate::surfaces::*;

pub use layer::{paint_layer, paint_layer_surface};

pub fn paint(
    runtime: &mut Runtime,
    renderer: &mut RenderEngine<WgpuBackend>,
    client: &dyn Backend,
    root: NodeHandle,
    cache: Option<&mut CachedLayout>,
) -> Result<CachedLayout, String> {
    // `MORF_FRAME_LOG=1` prints how long each frame of the primary surface
    // took on the CPU side, submission included, so a configuration that
    // feels slow can be read rather than guessed at.
    let started = frame_log_wanted().then(std::time::Instant::now);
    let painted = paint_layer(
        runtime,
        renderer,
        client,
        PRIMARY_LAYER,
        root,
        &runtime.layer_surface_config(),
        cache,
    );
    if let Some(started) = started {
        eprintln!(
            "{} frame on {} took {:.2} ms",
            crate::wake_plan::stamp(),
            std::thread::current().name().unwrap_or("?"),
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    painted
}

pub fn frame_log_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED
        .get_or_init(|| std::env::var_os("MORF_FRAME_LOG").is_some_and(|value| !value.is_empty()))
}

/// Lays out, masks, and renders the scene subtree of one layer surface.
///
/// Every layer surface derives its input region the same way: an explicit mask
/// when the configuration supplies one, and otherwise the live geometry of the
/// interactive items, recomputed here so it tracks the surface as it changes.
/// A layout, and what it was computed from.
///
/// Layout is the most expensive thing a frame does, and most frames change
/// nothing it reads — a colour easing, a morph advancing, an opacity fading.
/// Keeping what the last one was built from lets those frames reuse it.
#[derive(Clone)]
pub struct CachedLayout {
    pub layout: Layout,
    pub revision: u64,
    pub size: (u32, u32),
    pub scale_120: u32,
    /// The input region last handed to the compositor for this surface.
    ///
    /// It is double-buffered surface state, so it persists until it is set
    /// again — sending an identical one costs a region object, a round of
    /// protocol traffic and the derivation that produced it, and changes
    /// nothing.
    pub input: Vec<InputRect>,
    /// The backdrop-blur shapes last handed to the compositor.
    ///
    /// The *shapes*, not the rectangles they rasterise to, because rasterising
    /// is the expensive half — six milliseconds for a full-screen region in
    /// release — and comparing first means a swarm that has drifted less than
    /// the grid does no work at all rather than doing all of it and discovering
    /// the answer was the same.
    pub backdrop: Vec<Region>,
    /// The keyboard focus policy last sent for this surface.
    pub keyboard_focus: String,
}

impl CachedLayout {
    /// A layout that was computed without the cache: nothing reuses it.
    pub fn uncached(layout: Layout) -> Self {
        Self {
            layout,
            revision: u64::MAX,
            size: (0, 0),
            scale_120: 0,
            input: Vec::new(),
            backdrop: Vec::new(),
            keyboard_focus: String::new(),
        }
    }

    /// A soft reload replaces the scene, while its Wayland surface survives.
    /// Neither handles nor revision numbers can identify the new layout.
    pub fn invalidate_scene(&mut self) {
        self.layout = Layout::default();
        self.revision = u64::MAX;
        self.scale_120 = 0; // Force a full layout, never an incremental update.
        self.keyboard_focus.clear();
    }

    /// Whether this layout still describes the scene.
    ///
    /// Everything `Layout::compute` reads is either a property layout depends
    /// on inside the tree it lays out — which moves that tree's revision,
    /// [`morf_scene::Scene::layout_revision_of`] — or one of the two inputs
    /// handed to it. Nothing outside the tree is read: a binding that reads
    /// another surface's layout writes what it read into this tree, which
    /// moves this tree's revision like any other write.
    /// A surface that has resized, or that the compositor now presents at a
    /// different scale, has to be laid out again however still the scene is.
    pub fn still_valid(&self, revision: u64, size: (u32, u32), scale_120: u32) -> bool {
        self.revision == revision && self.size == size && self.scale_120 == scale_120
    }
}

impl std::ops::Deref for CachedLayout {
    type Target = Layout;

    fn deref(&self) -> &Layout {
        &self.layout
    }
}

/// The layout a paint draws, and whether it is a new one.
///
/// Taken out of the cache rather than copied, since the paint hands back a
/// new cache built round it. The cached layout itself when nothing it reads
/// has moved; otherwise that layout brought up to date, redoing only the
/// parts of the tree that moved ([`morf_layout::Layout::update_with`]); and
/// a whole pass when there is nothing to start from, the scale changed, or
/// `MORF_LAYOUT_FULL=1` asks for one every time (to compare the two).
pub fn layout_for(
    runtime: &mut Runtime,
    cache: Option<&mut CachedLayout>,
    root: NodeHandle,
    (revision, size, scale_120): (u64, (u32, u32), u32),
    text: &mut impl morf_layout::TextMeasurer,
) -> Result<(Layout, bool), String> {
    let available = Size {
        width: size.0 as f64,
        height: size.1 as f64,
    };
    let Some(cached) = cache else {
        return Ok((runtime.compute_layout(root, available, text)?, true));
    };
    if cached.still_valid(revision, size, scale_120) {
        return Ok((std::mem::take(&mut cached.layout), false));
    }
    // Whatever happens next, what is left in the cache is no longer this
    // revision's: a paint that fails part way must not find it valid.
    let since = cached.revision;
    cached.revision = u64::MAX;
    let mut layout = std::mem::take(&mut cached.layout);
    if cached.scale_120 != scale_120 || full_layout_wanted() {
        return Ok((runtime.compute_layout(root, available, text)?, true));
    }
    let started = std::time::Instant::now();
    runtime.update_layout(&mut layout, root, available, text)?;
    // A slow layout says what moved to need it.
    if frame_split_wanted() && started.elapsed() > std::time::Duration::from_millis(8) {
        let (count, lines) = runtime.layout_report(root, since, 8);
        eprintln!(
            "{} layout took {:.1} ms for {count} changed nodes:",
            morf_lua::profile::stamp(),
            started.elapsed().as_secs_f64() * 1000.0
        );
        for line in lines {
            eprintln!("    {line}");
        }
    }
    Ok((layout, true))
}

/// `MORF_LAYOUT_FULL=1`: lay every changed frame out whole, as before
/// layouts were brought up to date piecemeal. For measuring the difference.
fn full_layout_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| {
        std::env::var_os("MORF_LAYOUT_FULL").is_some_and(|value| !value.is_empty() && value != "0")
    })
}

/// Whether `MORF_FRAME_LOG=2` asks for each frame's detail.
pub fn frame_split_wanted() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("MORF_FRAME_LOG").is_ok_and(|value| value == "2"))
}

/// `MORF_FRAME_LOG=2`: where one frame's time went, stage by stage, for any
/// frame over 16 ms, or over `MORF_FRAME_SPLIT_MS` when that is set.
struct FrameSplit {
    on: bool,
    started: std::time::Instant,
    last: std::time::Instant,
    stages: Vec<(&'static str, f64)>,
}

impl FrameSplit {
    fn start() -> Self {
        let on = frame_split_wanted();
        let now = std::time::Instant::now();
        Self {
            on,
            started: now,
            last: now,
            stages: Vec::new(),
        }
    }

    fn mark(&mut self, stage: &'static str) {
        if !self.on {
            return;
        }
        let now = std::time::Instant::now();
        self.stages
            .push((stage, (now - self.last).as_secs_f64() * 1000.0));
        self.last = now;
    }

    fn finish(self) {
        static THRESHOLD: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
        let threshold = *THRESHOLD.get_or_init(|| {
            std::env::var("MORF_FRAME_SPLIT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(16.0)
        });
        let total = self.started.elapsed().as_secs_f64() * 1000.0;
        if !self.on || total < threshold {
            return;
        }
        let parts = self
            .stages
            .iter()
            .map(|(stage, ms)| format!("{stage} {ms:.1}"))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "{} frame split {total:.1} ms: {parts}",
            crate::wake_plan::stamp()
        );
    }
}

/// Stands in the input cache for "the configured mask, already sent".
///
/// A configured mask is not built from layout, so there is no rectangle list to
/// compare; what the cache needs is only something that is equal to itself and
/// unequal to any real region. The negative extent cannot arise from geometry,
/// which is floored and ceiled from a non-negative rectangle.
pub const MASK_SENTINEL: InputRect = InputRect {
    x: i32::MIN,
    y: i32::MIN,
    width: -1,
    height: -1,
};

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

pub fn paint_popup_surface(
    runtime: &mut Runtime,
    client: &dyn Backend,
    surface: &mut Window,
) -> Result<(), String> {
    paint_auxiliary_surface(AuxiliaryKind::Popup, runtime, client, surface)
}

pub fn paint_floating_surface(
    runtime: &mut Runtime,
    client: &dyn Backend,
    surface: &mut Window,
) -> Result<(), String> {
    paint_auxiliary_surface(AuxiliaryKind::Floating, runtime, client, surface)
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
    let (width, height) = physical_size((surface.width, surface.height), scale_120);
    if !renderer.backend_mut().declares_damage() {
        kind.damage(client, surface.id, width, height)?;
    }
    let damage = renderer
        .render(&scene, &layout, scale_120, |_| {})
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
        layout,
        revision,
        size,
        scale_120,
        input: Vec::new(),
        backdrop: Vec::new(),
        keyboard_focus: String::new(),
    });
    Ok(())
}

pub fn clock_text() -> String {
    jiff::Zoned::now().strftime("%H:%M:%S").to_string()
}

/// Draws a renderer's text in subpixels where this output and the
/// configuration allow it (morf-render's lcd.rs has the rules), and tells it
/// whether the surface is declared opaque. A change draws the next frame in
/// full.
pub fn apply_subpixel(
    renderer: &mut RenderEngine<WgpuBackend>,
    setting: &str,
    client: &dyn Backend,
    opaque: bool,
) {
    let text = client.own_output().and_then(|output| {
        morf_render::subpixel_text_for(
            setting,
            morf_render::font_subpixel(),
            output.subpixel,
            output.transform,
        )
    });
    let backend = renderer.backend_mut();
    let changed = backend.set_subpixel_text(text) | backend.set_opaque_surface(opaque);
    if changed {
        renderer.forget();
    }
}

/// Puts a renderer in the blend space a surface's configuration names.
///
/// Returns whether it changed, which rebuilt the renderer's target and
/// pipelines: the next frame is drawn in full, and whoever registered shaders
/// with it registers them again.
pub fn apply_blend(renderer: &mut RenderEngine<WgpuBackend>, blend: &str) -> bool {
    let blend = BlendSpace::parse(blend).unwrap_or_default();
    if !renderer.backend_mut().set_blend(blend) {
        return false;
    }
    renderer.forget();
    true
}
