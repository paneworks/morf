//! Raster, SVG, and XDG icon-theme loading with size-aware caches.

mod distance_field;
mod icons;
mod image_cache;
mod inline;
pub mod ops;
mod quantize;

pub use icons::IconResolver;
pub use image_cache::{ImageCache, ImageError};
pub use inline::{InlineSource, MAX_INLINE_BYTES, inline_source, is_inline_source};
pub use quantize::{ImageData, ImageRect, PaletteEntry, palette_of, quantize_colors};
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_ops;
