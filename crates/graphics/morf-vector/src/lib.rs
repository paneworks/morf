//! Closed outlines and SVG paths, and what a field needs from them.
//!
//! A letter and an icon are the same kind of thing: a set of closed loops with
//! curves in them. Everything here is about those loops and nothing about where
//! they came from — no font, no document, and above all no raster. An outline
//! measured into a field keeps its edge exact at any magnification, and two
//! outlines can be walked between point by point, which a picture of an outline
//! can do neither of.
//!
//! Whoever has the outline converts it into [`Step`]s once; from there a font
//! and an SVG are indistinguishable, and so are the shapes they make.

/// SVG documents read as outlines, not pictures.
pub mod svg;
mod contours;
mod corners;
mod flatten;
pub mod geometry;
pub mod geometry_named;
mod geometry_resample;
pub mod graph;
pub mod marks;
pub mod series;
mod series_kinds;
mod morph;
mod step;

pub use contours::{CONTOUR_POINTS, Contour, contour_of, contours, resample};
pub use corners::{Corner, corner_points, corners};
pub use flatten::{Segment, flatten};
pub use morph::{Paired, between, contour_points, pair_up, walk};
pub use step::Step;

#[cfg(test)]
mod tests;
