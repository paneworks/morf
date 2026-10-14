//! The glyph atlas growing a second page before it rebuilds.

use std::rc::Rc;

use morf_layout::Transform2D;
use morf_scene::Color;
use morf_text::{RasterContent, RasterGlyph};

use super::glyphs::{GLYPH_ATLAS_SIZE, GlyphKey, PreparedGlyph};
use crate::*;

fn glyph(cache_key: u64, side: u32) -> PreparedGlyph {
    PreparedGlyph {
        glyph: RasterGlyph {
            cache_key,
            x: 0.0,
            y: 0.0,
            width: side,
            height: side,
            draw_width: side as f32,
            draw_height: side as f32,
            content: RasterContent::Mask,
            tint: None,
            font_size: side as f32,
            data: Rc::new(vec![cache_key as u8; (side * side) as usize]),
        },
        morph: None,
        morph_progress: 0.0,
        ramp: 1.0,
        color: Color::rgba8(255, 255, 255, 255),
        color_overlay: Color::rgba8(0, 0, 0, 0),
        transform: Transform2D::IDENTITY,
        command_index: 0,
        field: [0.0; 4],
        outline_color: [0.0; 4],
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_full_atlas_grows_a_page_and_keeps_what_it_holds() {
    require_gpu!();
    let mut backend = pollster::block_on(WgpuBackend::new(8, 8)).unwrap();
    let queue = backend.queue.clone();
    let atlas = &mut backend.glyph_mask_atlas;
    // A quarter-page square per glyph: four fill the first page.
    let side = GLYPH_ATLAS_SIZE / 2 - 2;
    let first: Vec<_> = (1..=4).map(|key| glyph(key, side)).collect();
    atlas.prepare(&queue, &first).unwrap();
    assert_eq!(atlas.pages, 1);
    let more: Vec<_> = (5..=6).map(|key| glyph(key, side)).collect();
    atlas.prepare(&queue, &more).unwrap();
    assert_eq!(atlas.pages, 2, "the atlas grew instead of rebuilding");
    assert_eq!(atlas.height(), GLYPH_ATLAS_SIZE * 2);
    for key in 1..=4 {
        let entry = &atlas.entries[&GlyphKey::from_glyph(&first[key - 1].glyph)];
        assert!(entry.y < GLYPH_ATLAS_SIZE, "glyph {key} stayed on page one");
    }
    for prepared in &more {
        let entry = &atlas.entries[&GlyphKey::from_glyph(&prepared.glyph)];
        assert!(entry.y >= GLYPH_ATLAS_SIZE, "new glyphs went to page two");
    }
}
