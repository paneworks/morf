//! A bordered ClipRect inside an ancestor moved by a transform, drawn on a
//! real adapter: its contents show where the ancestor put it.

use super::field_tests::render_readback;
use crate::tests::translated_bordered_clip;
use crate::*;

fn at(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
    let offset = (y * 64 + x) * 4;
    pixels[offset..offset + 4].try_into().unwrap()
}

fn green(pixel: [u8; 4]) -> bool {
    pixel[1] > 200 && pixel[0] < 60 && pixel[2] < 60
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_bordered_clip_under_a_translated_ancestor_keeps_its_contents() {
    for radius in [0.0, 6.0] {
        let (scene, layout, _) = translated_bordered_clip(radius, 1.0);
        let list = DrawList::from_scene(&scene, &layout).unwrap();
        let pixels = render_readback(&list, 64);
        // The bar, 4 in and 4 down inside a clip that sits 32 down.
        assert!(
            green(at(&pixels, 12, 40)),
            "radius {radius}: the bar shows where the clip was moved: {:?}",
            at(&pixels, 12, 40),
        );
        // And is still cut at the clip's inner edge, not the untranslated one.
        assert!(!green(at(&pixels, 40, 40)), "radius {radius}: clipped");
        // Nothing is left behind at the place the clip would have been.
        assert!(!green(at(&pixels, 12, 8)), "radius {radius}: not above");
    }
}
