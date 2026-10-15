//! The layout a surface last drew, kept so a frame that changed nothing
//! layout reads can reuse it.

use morf_app::InputRect;
use morf_layout::{Layout, Size};
use morf_lua::Runtime;
use morf_scene::NodeHandle;
use morf_value::region::Region;

use super::frame_log::frame_split_wanted;

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
    /// The surface root's input stamp `input` was derived at: with the same
    /// layout and the same stamp, the region is the same.
    pub input_paint: u64,
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
            input_paint: u64::MAX,
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
