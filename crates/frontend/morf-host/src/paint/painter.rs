//! What a host paints with: the GPU, or -- with none (`morf check`, `morf
//! test`, a CI runner) -- nothing but a text system, so every window is laid
//! out, observed and given its input region as it would be, and never drawn.

use morf_render::{RenderEngine, WgpuBackend};
use morf_text::TextSystem;

pub enum Painter {
    Gpu(RenderEngine<WgpuBackend>),
    /// No GPU: the one text system every window is laid out with.
    Layout(TextSystem),
}

impl Painter {
    /// The text system layout measures with.
    pub fn text(&mut self) -> &mut TextSystem {
        match self {
            Self::Gpu(renderer) => renderer.backend_mut().text_system(),
            Self::Layout(text) => text,
        }
    }

    pub fn gpu(&mut self) -> Option<&mut RenderEngine<WgpuBackend>> {
        match self {
            Self::Gpu(renderer) => Some(renderer),
            Self::Layout(_) => None,
        }
    }

    /// The text system windows without a renderer of their own lay out
    /// with: only when nothing is drawn, since on the GPU each window has its
    /// own renderer and its own text.
    pub fn layout_only(&mut self) -> Option<&mut TextSystem> {
        match self {
            Self::Gpu(_) => None,
            Self::Layout(text) => Some(text),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if let Self::Gpu(renderer) = self {
            renderer.resize(width, height);
        }
    }
}
