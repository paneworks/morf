//! Regenerate only when intentionally changing canonical shape geometry.
use morf_outline::{geometry, geometry_named};
fn main() {
    for name in geometry_named::names() {
        let curves = geometry::curves(
            &geometry_named::outline(name).unwrap(),
            Some(geometry::SEGMENTS),
        )
        .unwrap();
        println!("{name}\t{}", geometry::path(&curves, 100.0).unwrap());
    }
}
