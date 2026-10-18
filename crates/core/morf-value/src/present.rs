//! How a frame drawn into a dmabuf reaches a window, said without naming a
//! window system: the renderer exports the buffers and fences them, and a
//! [`BufferSink`] -- the window's backend -- shows them.
//!
//! The renderer never sees the compositor and the backend never sees the
//! GPU: this is the whole of what they say to each other.

use std::any::Any;
use std::os::fd::BorrowedFd;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

/// One plane of a dmabuf, as the window system is told it.
#[derive(Clone, Copy, Debug)]
pub struct DmabufPlane<'a> {
    pub fd: BorrowedFd<'a>,
    pub offset: u32,
    pub stride: u32,
    pub modifier: u64,
}

/// A rectangle of a buffer that changed, in buffer pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Damage {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// A dmabuf the window system knows. Dropping it lets the window system
/// forget it.
pub trait SinkBuffer: Send + Sync {
    /// A number to tell buffers apart by in logs.
    fn label(&self) -> u32;
    /// For the sink that made it, to find its own type again.
    fn as_any(&self) -> &dyn Any;
}

/// A window that shows dmabufs.
pub trait BufferSink: Send + Sync {
    /// The modifiers the window system takes `fourcc` buffers with.
    fn modifiers(&self, fourcc: u32) -> Vec<u64>;
    /// Makes `plane` (a `size` buffer of format `fourcc`) a buffer the
    /// window system can show. `busy` is cleared whenever the window system
    /// lets go of it.
    fn import(
        &mut self,
        plane: DmabufPlane<'_>,
        size: (u32, u32),
        fourcc: u32,
        busy: Arc<AtomicBool>,
    ) -> Result<Box<dyn SinkBuffer>, String>;
    /// Shows `buffer`, `damage` being what changed since the last one.
    fn present(&mut self, buffer: &dyn SinkBuffer, damage: &[Damage]);
    /// Commits the window with whatever buffer it already shows.
    fn commit(&mut self);
    /// Hears whatever the window system sent: releases, mostly.
    fn dispatch(&mut self);
    /// Waits until the window system says something or `deadline` passes.
    /// Returns whether it said anything.
    fn wait(&mut self, deadline: Instant) -> bool;
    /// What the window system can be told about the buffers' colours.
    fn color_support(&self) -> ColorSupport {
        ColorSupport::default()
    }
    /// Says how the next buffers' values are to be read, from the next
    /// commit on. `Err` leaves them read as sRGB.
    fn set_encoding(&mut self, encoding: Encoding) -> Result<(), String> {
        match encoding {
            Encoding::Srgb => Ok(()),
            _ => Err("the window system has no colour management".to_owned()),
        }
    }
}

/// What a window system can be told about how a buffer's values are to be
/// read: the named parts of `wp_color_manager_v1` an HDR frame needs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ColorSupport {
    /// Image descriptions made from parameters.
    pub parametric: bool,
    /// A reference white of the client's choosing.
    pub luminances: bool,
    /// BT.2020 primaries.
    pub bt2020: bool,
    /// sRGB (BT.709) primaries.
    pub srgb_primaries: bool,
    /// The SMPTE ST 2084 (PQ) transfer function.
    pub pq: bool,
    /// The extended linear transfer function.
    pub ext_linear: bool,
}

impl ColorSupport {
    /// PQ over BT.2020 can be described.
    pub fn can_pq(&self) -> bool {
        self.parametric && self.pq && self.bt2020
    }

    /// Extended linear over sRGB primaries (scRGB) can be described.
    pub fn can_scrgb(&self) -> bool {
        self.parametric && self.ext_linear && self.srgb_primaries
    }

    /// Any HDR encoding at all.
    pub fn hdr(&self) -> bool {
        self.can_pq() || self.can_scrgb()
    }

    /// The HDR transfer functions there are, by name: `pq`, `ext_linear`.
    pub fn transfer_names(&self) -> Vec<&'static str> {
        [(self.can_pq(), "pq"), (self.can_scrgb(), "ext_linear")]
            .into_iter()
            .filter_map(|(can, name)| can.then_some(name))
            .collect()
    }
}

/// How a frame's values are to be read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Encoding {
    /// sRGB, as every buffer is without a description.
    #[default]
    Srgb,
    /// PQ over BT.2020, SDR white at `reference_white` cd/m².
    Pq { reference_white: u32 },
    /// Extended linear over sRGB primaries: 1.0 is SDR white (scRGB).
    ExtLinear,
}

impl Encoding {
    pub fn hdr(self) -> bool {
        self != Self::Srgb
    }
}

/// Set once any surface presents HDR: until then a node's `hdr` brightness
/// is left out, so an SDR frame looks as it was written.
static PRESENTING_HDR: AtomicBool = AtomicBool::new(false);

/// Records that a surface presents with `encoding`, when it is an HDR one.
pub fn mark_presenting(encoding: Encoding) {
    if encoding.hdr() {
        PRESENTING_HDR.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Whether some surface of this process presents HDR.
pub fn presenting_hdr() -> bool {
    PRESENTING_HDR.load(std::sync::atomic::Ordering::Relaxed)
}
