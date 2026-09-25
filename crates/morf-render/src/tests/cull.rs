//! A large field drawn as tiles: every pixel its surface reaches is in one.

use morf_region::{BlendProfile, Operation, Shape};

use super::*;

fn layer(x: f64, y: f64, width: f64, height: f64, operation: Operation) -> SdfLayer {
    SdfLayer {
        glyph: None,
        glyph_morph_to: None,
        svg_source: None,
        svg_source_morph_to: None,
        font_family: None,
        font_family_morph_to: None,
        bounds: Geometry {
            x,
            y,
            width,
            height,
        },
        color: Color::rgba8(255, 255, 255, 255),
        shape: Shape::Box,
        morph_to: Shape::Box,
        morph: 0.0,
        operation,
        blend: 18.0,
        rotation: 0.0,
        matrix: [1.0, 0.0, 0.0, 1.0],
        blend_group: 0,
        profile: BlendProfile::Circular,
        radii: [0.0; 4],
        points: 5.0,
        inner_radius: 0.5,
        thickness: 4.0,
        angle: 90.0,
    }
}

/// A 1920 by 1080 frame ten pixels thick, with a panel on its top edge, a
/// turned star in the middle and a sheared box near the left.
fn screen_frame() -> Vec<SdfLayer> {
    let mut hole = layer(10.0, 10.0, 1900.0, 1060.0, Operation::Subtract);
    hole.radii = [22.0; 4];
    let mut panel = layer(750.0, 10.0, 420.0, 150.0, Operation::SmoothUnion);
    panel.radii = [18.0; 4];
    panel.blend_group = 1;
    let mut star = layer(900.0, 500.0, 120.0, 120.0, Operation::SmoothUnion);
    star.shape = Shape::Star;
    star.rotation = 20.0;
    let mut sheared = layer(200.0, 400.0, 100.0, 60.0, Operation::Union);
    sheared.matrix = [1.0, 0.3, -0.2, 1.1];
    vec![
        layer(0.0, 0.0, 1920.0, 1080.0, Operation::Union),
        hole,
        panel,
        star,
        sheared,
    ]
}

fn tiles(layers: &[SdfLayer], scale: f64) -> Vec<crate::field::FieldTile> {
    let size = [(1920.0 * scale) as f32, (1080.0 * scale) as f32];
    crate::field::field_tiles(
        layers,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        },
        [0.0, 0.0, size[0], size[1]],
        scale,
        crate::field::Spill {
            edge: 2.0,
            shadow: None,
            solid: true,
        },
    )
    .expect("a hollow fullscreen frame is tiled")
}

#[test]
fn every_painted_pixel_of_a_tiled_frame_is_inside_a_tile() {
    for scale in [1.0, 1.5] {
        let layers = screen_frame();
        let tiles = tiles(&layers, scale);
        let covered = |x: f32, y: f32| {
            tiles.iter().any(|tile| {
                x >= tile.area[0] && x < tile.area[2] && y >= tile.area[1] && y < tile.area[3]
            })
        };
        let solid = |x: f32, y: f32| {
            tiles.iter().any(|tile| {
                tile.solid
                    && x >= tile.area[0]
                    && x < tile.area[2]
                    && y >= tile.area[1]
                    && y < tile.area[3]
            })
        };
        assert!(
            tiles.iter().any(|tile| tile.solid),
            "the frame's edges have insides"
        );
        let mut painted = 0;
        let step = 3;
        for py in (0..(1080.0 * scale) as u32).step_by(step) {
            for px in (0..(1920.0 * scale) as u32).step_by(step) {
                let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
                let logical = [x / scale as f32, y / scale as f32];
                let distance = composed_distance(&layers, logical);
                // A tile filled without walking the layers is deep inside.
                if solid(x, y) {
                    assert!(
                        distance < -2.0,
                        "{px},{py} at {scale}x is solid at {distance}"
                    );
                }
                if distance <= 2.0 {
                    painted += 1;
                    assert!(
                        covered(x, y),
                        "painted pixel {px},{py} at {scale}x is in no tile"
                    );
                }
            }
        }
        assert!(painted > 1000);
        let drawn: f32 = tiles
            .iter()
            .map(|tile| (tile.area[2] - tile.area[0]) * (tile.area[3] - tile.area[1]))
            .sum();
        let whole = (1920.0 * scale * 1080.0 * scale) as f32;
        assert!(
            drawn < whole * 0.45,
            "at {scale}x the tiles cover {:.0}% of the quad",
            drawn * 100.0 / whole
        );
    }
}

#[test]
fn a_small_field_or_one_with_a_letter_is_drawn_whole() {
    let mut layers = screen_frame();
    let small = crate::field::field_tiles(
        &layers,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 200.0,
        },
        [0.0, 0.0, 200.0, 200.0],
        1.0,
        crate::field::Spill {
            edge: 2.0,
            shadow: None,
            solid: true,
        },
    );
    assert!(small.is_none());
    layers[3].glyph = Some('8');
    layers[3].shape = Shape::Polygon;
    assert!(!composable(&layers));
}
