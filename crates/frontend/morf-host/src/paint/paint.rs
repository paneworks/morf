use morf_layout::{Layout, Size};
use morf_lua::{LayerSurfaceConfig, Runtime};
use morf_value::region::{Rect as RegionRect, Region};
use morf_render::{BlendSpace, RenderEngine, WgpuBackend};
use morf_scene::NodeHandle;
use morf_app::{InputRect, LayerClient, PRIMARY_LAYER, WindowId, physical_size};

use crate::{surface_layers::*, surfaces::*};
use morf_app::Backend as _;

pub fn paint(
    runtime: &mut Runtime,
    renderer: &mut RenderEngine<WgpuBackend>,
    client: &LayerClient,
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

pub fn paint_layer(
    runtime: &mut Runtime,
    renderer: &mut RenderEngine<WgpuBackend>,
    client: &LayerClient,
    layer: u64,
    root: NodeHandle,
    config: &LayerSurfaceConfig,
    mut cache: Option<&mut CachedLayout>,
) -> Result<CachedLayout, String> {
    let (width, height) = client
        .layer_logical_size(layer)
        .ok_or_else(|| "layer surface disappeared while painting".to_owned())?;
    // Read every paint like the keyboard focus below, so a configuration may
    // change it at any time; the renderer rebuilds only when it moved, and
    // its pipelines — the configuration's shaders among them — with it.
    if apply_blend(renderer, &config.blend) {
        crate::surface_run::register_shaders(runtime, renderer)?;
    }
    let scale_120 = client.layer_scale_120(layer).unwrap_or(120);
    apply_subpixel(renderer, &config.subpixel_text, client, config.opaque);
    // `morf.surface.keyboard_focus` is read every paint, so a configuration
    // may take the keyboard for a page and hand it back after, without a
    // second surface. Sent only when it differs from the last paint's.
    if cache
        .as_deref()
        .is_none_or(|cached| cached.keyboard_focus != config.keyboard_focus)
        && let Some(focus) = keyboard_focus_of(&config.keyboard_focus)
    {
        client.set_layer_keyboard_focus(layer, focus);
    }
    let mut split = FrameSplit::start();
    // This surface's tree's revision, not the scene's: a clock ticking on
    // another surface leaves this layout as it was.
    let revision = runtime.scene().layout_revision_of(root);
    let (layout, fresh) = layout_for(
        runtime,
        cache.as_deref_mut(),
        root,
        (revision, (width, height), scale_120),
        renderer.backend_mut(),
    )?;
    if fresh {
        // Only on a fresh layout: it is the one moment the answer can have
        // changed, and the cached one has already been looked at.
        split.mark("layout");
        runtime.lint_layout(&layout, root);
        split.mark("lint");
    }
    // Every frame, not only a fresh layout's: a caret that moved without the
    // text changing still has to be scrolled into view.
    runtime.sync_text_inputs(&layout, renderer.backend_mut().text_system());
    runtime.observe_stretch(&layout);
    split.mark("text inputs");
    let scene = runtime.scene();
    let input = if let Some(regions) = &config.input_regions {
        // A configured mask is a static surface setting — nothing animates it —
        // so rasterising it and re-sending it every paint asks the compositor
        // to rebuild an identical region sixty times a second. The branch below
        // has always deduped; this one opted out of the cache by returning an
        // empty vector, which also made every frame look like a change.
        //
        // The sentinel is what the cache compares: an empty vector would match
        // a surface that genuinely has no interactive area, so a shape that
        // stands for "the configured mask, unchanged" is stored instead.
        let input = vec![MASK_SENTINEL];
        if cache.as_deref().is_none_or(|cached| cached.input != input) {
            client
                .set_layer_composed_input_region(layer, regions)
                .map_err(|error| error.to_string())?;
        }
        input
    } else {
        let input = layout
            .input_geometry(&scene)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|geometry| {
                let left = geometry.x.floor() as i32;
                let top = geometry.y.floor() as i32;
                let right = (geometry.x + geometry.width).ceil() as i32;
                let bottom = (geometry.y + geometry.height).ceil() as i32;
                InputRect {
                    x: left,
                    y: top,
                    width: right - left,
                    height: bottom - top,
                }
            })
            .collect::<Vec<_>>();
        if cache.as_deref().is_none_or(|cached| cached.input != input) {
            client.set_input_region(WindowId::Layer(layer), Some(&input));
        }
        input
    };
    split.mark("input region");
    let mut backdrop = Vec::new();
    // Where the compositor should blur what is behind this surface. Nothing is
    // read back: the blur happens on the far side of this call, underneath a
    // surface that is about to be composited over it, and the only thing that
    // makes it visible is the alpha this configuration painted with.
    if client.supports_backdrop_blur() {
        let shapes: Vec<Region> = layout
            .backdrop_geometry(&scene)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|(geometry, radii)| Region {
                // Whole pixels, not grid cells. Quantising the *position*
                // here was worth nothing — a moving shape never compares equal
                // to its cached self whatever the grid, and a still one
                // compares equal without any — and it cost up to half a cell of
                // registration against the shape drawn over it, in a direction
                // that changed every frame.
                rect: RegionRect {
                    x: geometry.x.floor() as i32,
                    y: geometry.y.floor() as i32,
                    width: (geometry.width.ceil() as i32).max(0),
                    height: (geometry.height.ceil() as i32).max(0),
                },
                shape: morf_value::region::Shape::Box,
                params: morf_value::region::ShapeParams {
                    radii,
                    ..morf_value::region::ShapeParams::default()
                },
                ..Region::default()
            })
            .collect();
        // What is *sent* is the previous frame's shapes, not this frame's.
        //
        // We do not own the commit. Mesa's Vulkan display queue attaches and
        // commits the buffer on its own thread, at its own pace, and repeats a
        // buffer when it has nothing newer — so a region set here lands on
        // whichever commit happens next, which may carry a buffer older than
        // the geometry it was derived from. There is no pairing to rely on.
        //
        // Which direction that error falls in is not symmetric. A blur that
        // trails the shape by a frame is what a blur does; a blur that arrives
        // before the thing casting it is wrong in a way that reads instantly as
        // the effect predicting the motion. So the region is deliberately one
        // frame behind: it can only ever lag, and lag is the physical answer.
        let previous = cache
            .map(|cached| std::mem::take(&mut cached.backdrop))
            .unwrap_or_default();
        if previous != shapes && !previous.is_empty() {
            let rectangles =
                morf_value::region::build_scaled(width, height, &previous, morf_value::region::COVERED_EDGE_GRID)
                    .map_err(|error| error.to_string())?;
            client
                .set_layer_backdrop_region(layer, Some(&rectangles))
                .map_err(|error| error.to_string())?;
        }
        backdrop = shapes;
    }

    split.mark("backdrop region");
    client.request_frame(WindowId::Layer(layer));
    let surface = client
        .layer_surface(layer)
        .ok_or_else(|| "layer surface disappeared while painting".to_owned())?;
    // A backend presenting through its own buffers declares the damage with
    // the buffer itself (`WgpuBackend::declares_damage`).
    let declare = !renderer.backend_mut().declares_damage();
    let damage = renderer
        .render(&scene, &layout, scale_120, |damage| {
            if !declare {
                return;
            }
            // What actually changed, rather than the whole surface. A
            // compositor recomposites the area a client declares, so a
            // fullscreen overlay that declares everything costs a full screen
            // of blending every frame however little of it moved.
            for rect in damage {
                surface.damage_buffer(
                    rect.x as i32,
                    rect.y as i32,
                    rect.width as i32,
                    rect.height as i32,
                );
            }
        })
        .map_err(|error| error.to_string())?;
    split.mark("render");
    // What the frame repainted, beside how long it took: the one number that
    // says whether a change cost its own area or the whole surface.
    if frame_log_wanted() && !damage.is_empty() {
        let area: u64 = damage
            .iter()
            .map(|rect| u64::from(rect.width) * u64::from(rect.height))
            .sum();
        // With `MORF_FRAME_LOG=2`, where: the node that keeps a shell drawing
        // is found from the rectangle it repaints.
        let rects = if split.on {
            let listed = damage
                .iter()
                .take(4)
                .map(|rect| format!("{}x{}+{}+{}", rect.width, rect.height, rect.x, rect.y))
                .collect::<Vec<_>>()
                .join(" ");
            format!(": {listed}")
        } else {
            String::new()
        };
        eprintln!(
            "{} layer {layer} damaged {area} px in {} rect(s){rects}",
            crate::wake_plan::stamp(),
            damage.len()
        );
    }
    if damage.is_empty() {
        client.commit(WindowId::Layer(layer));
    }
    // After the frame is on its way, and only when nothing moves: text laid
    // out but hidden -- a preloaded panel -- gets its glyphs made now, so
    // the frame that shows it does not spend its time on them.
    if fresh && !runtime.has_motion() {
        renderer
            .backend_mut()
            .warm_hidden_text(&scene, &layout, root, scale_120);
        split.mark("warm hidden text");
    }
    drop(scene);
    // After the render: what the images became is known once they were drawn.
    runtime.sync_images(&layout, renderer.backend_mut().image_cache());
    runtime.observe_layout_with(&layout, fresh);
    split.mark("observe layout");
    split.finish();
    Ok(CachedLayout {
        layout,
        revision,
        size: (width, height),
        scale_120,
        input,
        backdrop,
        keyboard_focus: config.keyboard_focus.clone(),
    })
}

/// Paints one configured layer surface into its own renderer.
pub fn paint_layer_surface(
    runtime: &mut Runtime,
    client: &LayerClient,
    surface: &mut AuxiliarySurface,
) -> Result<(), String> {
    // Cleared here rather than at one of the two call sites, because there are
    // two: the frame callback honoured the flag and the main repaint block did
    // not, so an animating configured layer surface was painted twice for every
    // tick — once by each — and the flag it was supposed to be gated on was
    // never cleared by the one that ignored it.
    let Some(renderer) = &mut surface.renderer else {
        return Ok(());
    };
    // Still waiting for the last frame's callback: presenting again would
    // block a FIFO swapchain until it comes, and on a surface the compositor
    // is not showing it never does. Kept owed; the callback paints it. A
    // surface that has never painted is exempt, since it is not mapped until
    // it does and an unmapped surface's callback waits for that.
    if surface.layout.is_some()
        && client
            .layer_frame_wait(window_layer_id(surface.id))
            .is_some()
    {
        surface.needs_paint = true;
        return Ok(());
    }
    surface.needs_paint = false;
    let config = surface
        .layer_config
        .clone()
        .ok_or_else(|| "layer surface lost its configuration".to_owned())?;
    let painted = paint_layer(
        runtime,
        renderer,
        client,
        window_layer_id(surface.id),
        surface.root,
        &config,
        surface.layout.as_mut(),
    )?;
    // A binding on this tree's layout geometry (`layout_width`, ...) hears
    // the frame as it is observed, after the render, and may move the tree
    // again: centred on its own measured width, a capture toolbar was drawn
    // where the first frame put it, off centre, until something else
    // repainted a surface that never does by itself. One more paint is owed;
    // the frame callback `paint_layer` asked for makes it.
    surface.needs_paint = runtime.scene().layout_revision_of(surface.root) != painted.revision;
    surface.layout = Some(painted);
    Ok(())
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

    pub fn request_frame(self, client: &LayerClient, id: u64) {
        match self {
            Self::Popup => client.request_frame(WindowId::Popup(id)),
            Self::Floating => client.request_frame(WindowId::Toplevel(id)),
        }
    }

    /// Declares the whole surface damaged, ahead of the render that fills it.
    pub fn damage(
        self,
        client: &LayerClient,
        id: u64,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let surface = match self {
            Self::Popup => client.popup_surface(id),
            Self::Floating => client.floating_surface(id),
        }
        .ok_or_else(|| format!("{} surface disappeared while painting", self.name()))?;
        surface.damage_buffer(0, 0, width as i32, height as i32);
        Ok(())
    }

    pub fn commit(self, client: &LayerClient, id: u64) {
        let surface = match self {
            Self::Popup => client.popup_surface(id),
            Self::Floating => client.floating_surface(id),
        };
        if let Some(surface) = surface {
            surface.commit();
        }
    }
}

pub fn paint_popup_surface(
    runtime: &mut Runtime,
    client: &LayerClient,
    surface: &mut AuxiliarySurface,
) -> Result<(), String> {
    paint_auxiliary_surface(AuxiliaryKind::Popup, runtime, client, surface)
}

pub fn paint_floating_surface(
    runtime: &mut Runtime,
    client: &LayerClient,
    surface: &mut AuxiliarySurface,
) -> Result<(), String> {
    paint_auxiliary_surface(AuxiliaryKind::Floating, runtime, client, surface)
}

/// Paints one popup or floating surface.
pub fn paint_auxiliary_surface(
    kind: AuxiliaryKind,
    runtime: &mut Runtime,
    client: &LayerClient,
    surface: &mut AuxiliarySurface,
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
    client: &LayerClient,
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
