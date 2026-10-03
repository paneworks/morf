use morf_reactive::SignalId;
use slotmap::{SlotMap, new_key_type};
use std::collections::{BTreeMap, HashMap};

use crate::{animation::*, groups::*, hashing::*};

new_key_type! {
    pub(crate) struct NodeId;
}

/// A generational scene node handle safe to retain outside the arena.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NodeHandle(pub(crate) NodeId);

impl NodeHandle {
    pub(crate) fn id(self) -> NodeId {
        self.0
    }
}

/// Element kinds implemented by the first scene milestone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Element {
    /// Non-painting container.
    Item,
    /// Single-child container applying configurable margins.
    Inset,
    /// Rounded rectangle primitive.
    Rect,
    /// Rounded rectangle that clips content and overlays its border.
    ClipRect,
    /// Shaped text primitive.
    Text,
    /// Editable text: a caret, a selection, and the keys and pointer that move
    /// them. Single line unless `multiline` says otherwise.
    TextInput,
    /// Raster or SVG image primitive.
    Image,
    /// XDG icon-theme image primitive.
    Icon,
    /// Signed-distance field composed from its `SdfShape` children.
    Sdf,
    /// One analytic distance field inside an [`Element::Sdf`].
    ///
    /// Never painted on its own: the parent reads its geometry, its shape and
    /// its combining operation, and resolves the whole composition in one
    /// fragment shader. Because the layer is an ordinary node, every number it
    /// carries animates through the same behaviors as any other property.
    SdfShape,
    /// A vector outline: SVG path data, filled and stroked.
    ///
    /// Every number and colour on it animates like any other property, and
    /// its outline is drawn at the pixel size it covers, so it stays crisp
    /// however it is scaled. The outline itself morphs into `morph_to` when
    /// the two have the same run of segments.
    Path,
    /// Pointer and focus event target with no visual output.
    MouseArea,
    /// Target for drags from other applications, with no visual output.
    DropArea,
    /// Sequential horizontal positioner.
    Row,
    /// Sequential vertical positioner.
    Column,
    /// Fixed-column two-dimensional positioner.
    Grid,
    /// Clipped viewport over movable content.
    Flickable,
    /// Non-painting container for a lazily constructed child.
    Loader,
    /// Non-painting periodic callback object.
    Timer,
    /// A flexbox container: its children are placed by grow, shrink, basis,
    /// wrap and alignment rather than by their own `x` and `y`.
    Flex,
    /// A container whose measure and placement are functions the
    /// configuration wrote.
    Custom,
    /// A terminal emulator: a program on a pseudo-terminal, its screen drawn
    /// as a grid of cells, the keyboard and the pointer going to it.
    Terminal,
}

impl Element {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Item => "Item",
            Self::Inset => "Inset",
            Self::Rect => "Rect",
            Self::ClipRect => "ClipRect",
            Self::Text => "Text",
            Self::TextInput => "TextInput",
            Self::Image => "Image",
            Self::Icon => "Icon",
            Self::Sdf => "Sdf",
            Self::SdfShape => "SdfShape",
            Self::Path => "Path",
            Self::MouseArea => "MouseArea",
            Self::DropArea => "DropArea",
            Self::Row => "Row",
            Self::Column => "Column",
            Self::Grid => "Grid",
            Self::Flickable => "Flickable",
            Self::Loader => "Loader",
            Self::Timer => "Timer",
            Self::Flex => "Flex",
            Self::Custom => "Layout",
            Self::Terminal => "Terminal",
        }
    }
}

pub use morf_value::Color;

/// Values stored in reactive element properties.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// No value.
    Nil,
    /// Boolean value.
    Bool(bool),
    /// Floating-point number.
    Number(f64),
    /// UTF-8 string.
    String(String),
    /// Normalized RGBA colour.
    Color(Color),
    /// Ordered sequence used by declarative data.
    List(Vec<Value>),
    /// String-keyed declarative data such as anchors.
    Map(BTreeMap<String, Value>),
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Number(value as f64)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

/// Scene arena and its property signal graph.
pub struct Scene {
    pub(crate) nodes: SlotMap<NodeId, Node>,
    /// Shaders attached to nodes, by node.
    ///
    /// A side table rather than node properties: property names are `&'static
    /// str`, so a per-shader parameter name would have to be leaked, and giving
    /// every element a fixed set of numbered slots would make every rectangle
    /// in the scene carry two signals per slot whether or not it has a shader.
    /// A shader is rare; it should cost nothing when absent.
    pub(crate) shaders: FastMap<NodeId, NodeShader>,
    /// What each `Terminal` node's screen shows, by node: a side table for
    /// the same reason `shaders` is one. See [`crate::TerminalScreen`].
    pub(crate) terminal_screens: FastMap<NodeId, std::sync::Arc<crate::TerminalScreen>>,
    pub(crate) properties: crate::property_store::PropertyStore,
    pub(crate) behaviors: FastMap<PropertyKey, Behavior>,
    pub(crate) animations: FastMap<PropertyKey, Animation>,
    pub(crate) physics: FastMap<PropertyKey, PhysicsAnimation>,
    pub(crate) physics_specs: FastMap<PropertyKey, Physics>,
    pub(crate) paused_physics: FastSet<PropertyKey>,
    pub(crate) events: Vec<AnimationEvent>,
    pub(crate) groups: HashMap<GroupId, RunningGroup>,
    pub(crate) group_events: Vec<GroupEvent>,
    pub(crate) next_group: u64,
    /// Bumped whenever something that layout reads changes.
    ///
    /// Layout is the most expensive thing a frame does — it walks the whole
    /// tree measuring, resolving anchors and placing children — and most frames
    /// change nothing it reads. A colour easing, a morph advancing, an opacity
    /// fading: none of them move a box. Recording when the geometry last
    /// actually moved lets a paint reuse the layout it already has.
    pub(crate) layout_revision: u64,
    /// The layout revision each tree last moved at, keyed by its root.
    ///
    /// A shell draws several trees — the bar, the desk, a settings window —
    /// each on its own surface, and a clock ticking on the bar has nothing to
    /// do with the settings window's layout. With one revision for the whole
    /// scene every change laid out every surface again. A change is recorded
    /// against the root of the tree it happened in, so a surface re-lays out
    /// only when its own tree moved; see [`Scene::layout_revision_of`].
    pub(crate) root_revisions: FastMap<NodeId, u64>,
    /// The layout revision each tree last lost a node at, by its root: a
    /// node removed, or moved out to somewhere else. An incremental layout
    /// cannot tell which of the nodes it knows are gone, so a loss since it
    /// was made sends it back to a whole pass.
    pub(crate) detached_revisions: FastMap<NodeId, u64>,
    /// How fast motion runs: 1 is real time, 0 finishes everything at once.
    pub(crate) motion_scale: f64,
    /// See [`Scene::set_start_on_tick`].
    pub(crate) start_on_tick: bool,
    /// Nodes destroyed since anyone last asked.
    ///
    /// Every cache keyed on a node lives outside this crate — shaped text
    /// buffers in `morf-text`, transforms in `morf-lua`, atlases in the GPU
    /// backend — and none of them can see a node die. Without a signal that
    /// crosses the boundary they grow for the life of the process, and each one
    /// grew its own eviction method that nothing ever called. This is that
    /// signal: the scene records what it destroyed and whoever drives the frame
    /// hands the list to everything holding node-keyed state.
    pub(crate) removed: Vec<NodeHandle>,
    /// The node each field layer follows, by layer: see [`Scene::set_track`].
    /// A side table, as `shaders` is, because it names a node and a property
    /// value cannot, and because almost no node has one.
    pub(crate) tracks: FastMap<NodeId, NodeHandle>,
    /// The node whose subtree masks each masked node: see [`Scene::set_mask`].
    pub(crate) masks: FastMap<NodeId, NodeHandle>,
    /// The other way round: the node each mask masks.
    pub(crate) mask_owners: FastMap<NodeId, NodeHandle>,
    /// Squash-and-stretch springs, by node: see [`crate::Stretch`].
    pub(crate) stretch: FastMap<NodeId, crate::stretch::StretchState>,
    /// Seconds of motion ticked so far, the clock stretch velocities are
    /// measured against.
    pub(crate) stretch_clock: f64,
    /// How nodes that declared one leave: see [`crate::ExitSpec`].
    pub(crate) exit_specs: FastMap<NodeId, crate::ExitSpec>,
    /// Where each node that declared an exit was last placed, relative to
    /// its parent: the box it keeps when it starts to leave. Noted by the
    /// layout through a shared reference, hence the cells.
    pub(crate) exit_placed: FastMap<NodeId, std::cell::Cell<Option<[f64; 4]>>>,
    /// Nodes on their way out, drawn but out of the flow.
    pub(crate) exiting: FastMap<NodeId, crate::exit::Exiting>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PropertyKey {
    pub(crate) node: NodeId,
    pub(crate) property: &'static str,
}

/// A compiled shader attached to a node, and the values it was given.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeShader {
    /// Which registered program, by the hash of its generated WGSL.
    pub program: u64,
    /// Parameter values, flattened in declaration order.
    pub params: Vec<f32>,
    /// Values for the shader's data blocks, one run per block in binding
    /// order. Read-only to the shader; the configuration owns them.
    pub data: Vec<Vec<f32>>,
    /// Whether the shader reads what is rendered underneath, and so runs in
    /// the composite pass over a layer rather than in the field pass.
    pub samples_behind: bool,
    /// Whether the shader decides its own coverage rather than colouring what
    /// the node's own shape already covered.
    ///
    /// It travels with the attachment because it changes the *geometry* the
    /// fragment stage walks, not just the colour: a shader that owns its
    /// coverage has to be given the node's whole rectangle, or it paints only
    /// where the shape it replaced would have been.
    pub owns_coverage: bool,
}

pub(crate) struct Node {
    pub(crate) element: Element,
    pub(crate) parent: Option<NodeId>,
    // Handles rather than raw ids so the children can be handed out as a
    // borrowed slice. Every tree walk in the engine asks for them — layout does
    // it five times per node, and paint and hit testing once each — so building
    // a fresh Vec per call put hundreds of allocations in every frame.
    pub(crate) children: Vec<NodeHandle>,
    pub(crate) properties: FastMap<&'static str, PropertySlot>,
    /// When layout last had a reason to look at this node, and at what.
    pub(crate) stamps: LayoutStamps,
}

/// The layout revisions a node was last touched at, which is what lets a
/// layout computed at one revision redo only what has moved since.
///
/// Each is a value of [`Scene::layout_revision`] at the time: a layout made
/// at revision `r` knows a node is as it left it when all three are `<= r`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LayoutStamps {
    /// Something layout reads on the node itself changed: a property, or
    /// its list of children.
    pub own: u64,
    /// The node or something under it changed — the newest `own` in its
    /// subtree, or its `attached`. A subtree whose `subtree` is old can be
    /// taken from the last layout whole.
    pub subtree: u64,
    /// The node joined the tree it is in: nothing the last layout says
    /// about it or anything under it can be trusted.
    pub attached: u64,
}

#[derive(Clone, Copy)]
pub(crate) struct PropertySlot {
    pub(crate) current: SignalId,
    pub(crate) target: SignalId,
    pub(crate) kind: PropertyType,
}

#[derive(Clone, Copy)]
pub(crate) enum PropertyType {
    Any,
    Bool,
    Number,
    String,
    Color,
}
