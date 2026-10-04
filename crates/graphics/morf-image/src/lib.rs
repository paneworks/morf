//! Raster, SVG, and XDG icon-theme loading with size-aware caches.

pub mod animation;
pub mod annotation;
mod annotation_geometry;
pub mod canvas;
mod distance_field;
mod icons;
mod image_cache;
mod inline;
pub mod ops;
mod ops_memory;
mod ops_queries;
pub mod preview;
pub mod published;
mod quantize;
mod svg_fonts;
pub mod xdg;

pub use animation::Animation;
pub use icons::IconResolver;
pub use image_cache::{ImageCache, ImageError, MAX_ANIMATIONS, MAX_ANIMATIONS_BYTES};
pub use inline::{InlineSource, MAX_INLINE_BYTES, inline_source, is_inline_source};
pub use quantize::{ImageData, ImageRect, PaletteEntry, palette_of, quantize_colors};
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_ops;

#[cfg(test)]
mod tests_annotation;
