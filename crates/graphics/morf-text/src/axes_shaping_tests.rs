// Axes that change advances reach shaping: what is measured is what is drawn.

use std::path::Path;

use cosmic_text::skrifa::{self, MetadataProvider as _};
use morf_layout::{FontAxis, OpticalSizing, Size, TextMeasurer, TextOptions, TextStyle};
use morf_scene::{Element, Scene};

use crate::TextSystem;
use crate::variations_tests::{options, text_font_with};

fn with_sizing(mut options: TextOptions, sizing: OpticalSizing) -> TextOptions {
    options.style.optical_sizing = sizing;
    options
}

/// The advances of `word` at `settings`, summed straight from the font's
/// metrics (`hmtx` and `HVAR`) with no shaper involved.
fn advances(path: &Path, word: &str, size: f32, settings: &[(&[u8; 4], f32)]) -> f32 {
    let data = std::fs::read(path).unwrap();
    let font = skrifa::FontRef::new(&data).unwrap();
    let location = font.axes().location(
        settings
            .iter()
            .map(|(tag, value)| (skrifa::Tag::new(tag), *value)),
    );
    let metrics = font.glyph_metrics(skrifa::instance::Size::new(size), &location);
    let charmap = font.charmap();
    word.chars()
        .map(|c| metrics.advance_width(charmap.map(c).unwrap()).unwrap())
        .sum()
}

/// Where the ink of a node's drawn glyphs ends on the right, in pixels.
fn ink_right(text: &mut TextSystem, node: morf_scene::NodeHandle) -> f32 {
    text.rasterize(node, (0.0, 0.0), 1.0, false)
        .iter()
        .map(|glyph| glyph.x + glyph.draw_width)
        .fold(0.0, f32::max)
}

fn close(a: f64, b: f32, tolerance: f32) -> bool {
    (a as f32 - b).abs() <= tolerance
}

#[test]
fn a_wdth_axis_moves_the_glyphs_it_measures_and_draws() {
    let Some((family, path)) = text_font_with(b"wdth") else {
        eprintln!("no text face with a wdth axis here; skipped");
        return;
    };
    let mut scene = Scene::new();
    let narrow = scene.create(Element::Text);
    let wide = scene.create(Element::Text);
    let mut text = TextSystem::new();
    let word = "nnnnnnnn";
    let measure = |text: &mut TextSystem, node, wdth: f32| {
        text.measure(
            node,
            word,
            &family,
            24.0,
            with_sizing(options(&path, &[(b"wdth", wdth)]), OpticalSizing::None),
        )
    };
    let thin: Size = measure(&mut text, narrow, 50.0);
    let broad: Size = measure(&mut text, wide, 151.0);
    assert!(
        broad.width > thin.width * 1.3,
        "{family}: {} against {}",
        broad.width,
        thin.width
    );
    // The widths are the font's own advances at those points.
    for (size, wdth) in [(thin, 50.0), (broad, 151.0)] {
        let expected = advances(&path, word, 24.0, &[(b"wdth", wdth)]);
        assert!(
            close(size.width, expected, expected * 0.01),
            "{family} at wdth {wdth}: measured {} against the font's {expected}",
            size.width
        );
    }
    // And the glyphs are drawn where they were measured: the last one's ink
    // ends inside the measured width, a side bearing short of it.
    for (node, size) in [(narrow, thin), (wide, broad)] {
        let right = ink_right(&mut text, node);
        assert!(
            right <= size.width as f32 + 1.0 && right >= size.width as f32 - 24.0 * 0.25,
            "{family}: ink ends at {right}, measured {}",
            size.width
        );
    }
}

#[test]
fn optical_size_follows_the_font_size_unless_it_is_turned_off() {
    let Some((family, path)) = text_font_with(b"opsz") else {
        eprintln!("no text face with an opsz axis here; skipped");
        return;
    };
    let range = crate::file_axes(&path)
        .into_iter()
        .find(|axis| &axis.tag == b"opsz")
        .unwrap();
    // A size away from the axis's default, so following it shows.
    let size = if range.default > range.min {
        (range.min.max(8.0) + range.default) / 2.0
    } else {
        (range.default + range.max) / 2.0
    }
    .round();
    let mut scene = Scene::new();
    let [auto, none, named] = [(); 3].map(|()| scene.create(Element::Text));
    let mut text = TextSystem::new();
    let word = "nnnnnnnnnnnn";
    let followed = text.measure(auto, word, &family, size.into(), options(&path, &[]));
    let fixed = text.measure(
        none,
        word,
        &family,
        size.into(),
        with_sizing(options(&path, &[]), OpticalSizing::None),
    );
    let asked = text.measure(
        named,
        word,
        &family,
        size.into(),
        with_sizing(options(&path, &[(b"opsz", size)]), OpticalSizing::None),
    );
    assert!(
        (followed.width - fixed.width).abs() > 0.5,
        "{family} at {size}px: {} with, {} without",
        followed.width,
        fixed.width
    );
    assert_eq!(followed, asked, "automatic is opsz at the size in pixels");
    let expected = advances(&path, word, size, &[(b"opsz", size)]);
    assert!(
        close(followed.width, expected, expected * 0.01),
        "{family}: {} against the font's {expected}",
        followed.width
    );
    let right = ink_right(&mut text, auto);
    assert!(
        right <= followed.width as f32 + 1.0,
        "{family}: ink ends at {right}, measured {}",
        followed.width
    );
}

#[test]
fn the_axes_a_style_is_set_at() {
    let axis = |tag: &[u8; 4], value| FontAxis { tag: *tag, value };
    let style = TextStyle {
        axes: vec![axis(b"wdth", 80.0), axis(b"wght", 700.0)],
        ..TextStyle::default()
    };
    assert_eq!(
        style.variation_axes(12.0),
        vec![axis(b"wdth", 80.0), axis(b"opsz", 12.0)],
        "wght is the weight's; opsz follows the size"
    );
    let named = TextStyle {
        axes: vec![axis(b"opsz", 30.0)],
        ..TextStyle::default()
    };
    assert_eq!(named.variation_axes(12.0), vec![axis(b"opsz", 30.0)]);
    let off = TextStyle {
        optical_sizing: OpticalSizing::None,
        ..TextStyle::default()
    };
    assert_eq!(off.variation_axes(12.0), Vec::new());
    assert_ne!(style.key(), TextStyle::default().key(), "axes are layout");
    assert_ne!(off.key(), TextStyle::default().key());
}

#[test]
fn an_animated_wdth_is_shaped_at_a_bounded_number_of_points() {
    let Some((family, path)) = text_font_with(b"wdth") else {
        eprintln!("no text face with a wdth axis here; skipped");
        return;
    };
    let range = crate::file_axes(&path)
        .into_iter()
        .find(|axis| &axis.tag == b"wdth")
        .unwrap();
    let mut scene = Scene::new();
    let node = scene.create(Element::Text);
    let mut text = TextSystem::new();
    let mut widths = std::collections::HashSet::new();
    let mut pictures = std::collections::HashSet::new();
    for frame in 0..=1000 {
        let value = range.min + (range.max - range.min) * frame as f32 / 1000.0;
        let size = text.measure(
            node,
            "n",
            &family,
            24.0,
            options(&path, &[(b"wdth", value)]),
        );
        widths.insert(size.width.to_bits());
        for glyph in text.rasterize(node, (0.0, 0.0), 1.0, false) {
            pictures.insert(glyph.cache_key);
        }
    }
    assert!(widths.len() > 8, "it did move: {} widths", widths.len());
    assert!(widths.len() <= 129, "{} widths", widths.len());
    assert!(pictures.len() <= 129, "{} pictures", pictures.len());
}
