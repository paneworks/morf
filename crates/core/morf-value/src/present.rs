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
}
