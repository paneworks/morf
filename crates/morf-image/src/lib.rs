//! Raster, SVG, and XDG icon-theme loading with size-aware caches.

pub mod animation;
mod distance_field;
mod icons;
mod image_cache;
mod inline;
pub mod ops;
pub mod published;
mod quantize;

pub use animation::Animation;
pub use icons::IconResolver;
pub use image_cache::{ImageCache, ImageError, MAX_ANIMATIONS, MAX_ANIMATIONS_BYTES};
pub use inline::{InlineSource, MAX_INLINE_BYTES, inline_source, is_inline_source};
pub use quantize::{ImageData, ImageRect, PaletteEntry, palette_of, quantize_colors};
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_ops;
