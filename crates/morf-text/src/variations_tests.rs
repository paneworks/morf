use std::path::PathBuf;

use cosmic_text::fontdb;
use morf_layout::{FontAxis, TextMeasurer, TextOptions, TextStyle};
use morf_scene::{Element, Scene};

use crate::TextSystem;

/// A font with the axis `tag`, as a family and the file to load it from:
/// `MORF_TEST_VARIABLE_FONT`, Inter from the Nix store, or an installed
/// variable family. `None` skips the test on a machine without one.
pub(crate) fn font_with(tag: &[u8; 4]) -> Option<(String, PathBuf)> {
    let mut paths: Vec<PathBuf> = std::env::var_os("MORF_TEST_VARIABLE_FONT")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    paths.push(PathBuf::from(
        "/nix/store/shisxgcl8mqahsi4wxr67s44pdwxkjgp-inter-4.1/share/fonts/truetype/InterVariable.ttf",
    ));
    for family in [
        "Material Symbols Rounded",
        "Inter Variable",
        "Inter",
        "Adwaita Sans",
    ] {
        paths.extend(crate::family_files(family));
    }
    paths.into_iter().find_map(|path| {
        if !crate::file_axes(&path).iter().any(|axis| &axis.tag == tag) {
            return None;
        }
        let mut database = fontdb::Database::new();
        database.load_font_file(&path).ok()?;
        let family = database.faces().next()?.families.first()?.0.clone();
        Some((family, path))
    })
}

fn options(path: &std::path::Path, axes: &[(&[u8; 4], f32)]) -> TextOptions {
    TextOptions {
        font_source: Some(path.to_string_lossy().into_owned()),
        style: TextStyle {
            axes: axes
                .iter()
                .map(|(tag, value)| FontAxis {
                    tag: **tag,
                    value: *value,
                })
                .collect(),
            ..TextStyle::default()
        },
        ..TextOptions::default()
    }
}

/// Coverage of a node's glyphs, rasterised directly, summed, and their keys.
fn ink(text: &mut TextSystem, node: morf_scene::NodeHandle) -> (u64, Vec<u64>) {
    let glyphs = text.rasterize(node, (0.0, 0.0), 1.0, false);
    let ink = glyphs
        .iter()
        .flat_map(|glyph| glyph.data.iter())
        .map(|&byte| u64::from(byte))
        .sum();
    (ink, glyphs.iter().map(|glyph| glyph.cache_key).collect())
}

#[test]
fn axes_parse_from_a_map_of_tags_and_refuse_anything_else() {
    use morf_scene::Value;
    let map = |pairs: &[(&str, Value)]| {
        Value::Map(
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        )
    };
    let axes = FontAxis::parse_map(&map(&[
        ("FILL", Value::Number(1.0)),
        ("wght", Value::Number(500.0)),
    ]))
    .unwrap();
    assert_eq!(
        axes,
        vec![
            FontAxis {
                tag: *b"FILL",
                value: 1.0
            },
            FontAxis {
                tag: *b"wght",
                value: 500.0
            },
        ]
    );
    assert!(FontAxis::parse_map(&map(&[("weight", Value::Number(1.0))])).is_err());
    assert!(FontAxis::parse_map(&map(&[("FILL", Value::String("x".into()))])).is_err());
    assert_eq!(FontAxis::parse_map(&Value::Nil).unwrap(), Vec::new());
}

#[test]
fn a_wght_axis_is_the_weight_shaping_uses() {
    let Some((family, path)) = font_with(b"wght") else {
        eprintln!("no variable font with a wght axis here; skipped");
        return;
    };
    let mut scene = Scene::new();
    let light = scene.create(Element::Text);
    let heavy = scene.create(Element::Text);
    let mut text = TextSystem::new();
    let word = "Wombat";
    let thin = text.measure(
        light,
        word,
        &family,
        32.0,
        options(&path, &[(b"wght", 100.0)]),
    );
    let bold = text.measure(
        heavy,
        word,
        &family,
        32.0,
        options(&path, &[(b"wght", 900.0)]),
    );
    assert_eq!(
        text.buffers[&crate::BufferKey::own(heavy)]
            .input
            .as_ref()
            .unwrap()
            .font_weight,
        900,
        "{family}"
    );
    assert!(
        bold.width > thin.width,
        "{family}: heavier is wider, {} against {}",
        bold.width,
        thin.width
    );
    let (thin_ink, _) = ink(&mut text, light);
    let (bold_ink, _) = ink(&mut text, heavy);
    assert!(
        bold_ink > thin_ink * 3 / 2,
        "{family}: {bold_ink} against {thin_ink}"
    );
}

#[test]
fn a_fill_axis_fills_an_icon_and_keys_the_picture_apart() {
    let Some((family, path)) = font_with(b"FILL") else {
        eprintln!("no variable font with a FILL axis here; skipped");
        return;
    };
    let mut scene = Scene::new();
    let node = scene.create(Element::Text);
    let mut text = TextSystem::new();
    // `home`, by its ligature: Material Symbols draws the icon for the word.
    let icon = "home";
    let measure = |text: &mut TextSystem, fill: f32| {
        text.measure(
            node,
            icon,
            &family,
            48.0,
            options(&path, &[(b"FILL", fill)]),
        )
    };
    let outline_size = measure(&mut text, 0.0);
    let (outline, outline_keys) = ink(&mut text, node);
    let filled_size = measure(&mut text, 1.0);
    let (filled, filled_keys) = ink(&mut text, node);
    assert_eq!(outline_size, filled_size, "filling moves nothing");
    assert_ne!(outline_keys, filled_keys, "the two are two pictures");
    assert!(
        filled > outline * 3 / 2,
        "{family}: filled has more ink, {filled} against {outline}"
    );
    // The distance field follows the axis the same way.
    measure(&mut text, 0.0);
    let hollow = text.rasterize(node, (0.0, 0.0), 1.0, true);
    measure(&mut text, 1.0);
    let solid = text.rasterize(node, (0.0, 0.0), 1.0, true);
    assert_ne!(
        hollow
            .iter()
            .map(|glyph| glyph.cache_key)
            .collect::<Vec<_>>(),
        solid
            .iter()
            .map(|glyph| glyph.cache_key)
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_animated_axis_asks_for_a_bounded_number_of_pictures() {
    let found = [*b"FILL", *b"wght"]
        .into_iter()
        .find_map(|tag| font_with(&tag).map(|found| (tag, found)));
    let Some((tag, (family, path))) = found else {
        eprintln!("no variable font here; skipped");
        return;
    };
    let range = crate::file_axes(&path)
        .into_iter()
        .find(|axis| axis.tag == tag)
        .unwrap();
    let mut scene = Scene::new();
    let node = scene.create(Element::Text);
    let mut text = TextSystem::new();
    let mut keys = std::collections::HashSet::new();
    // A thousand frames of an animation across the whole axis. `wght` is
    // shaping's and steps by whole weights; the others are the rasteriser's.
    let other = if tag == *b"wght" { *b"XXXX" } else { tag };
    for frame in 0..=1000 {
        let value = range.min + (range.max - range.min) * frame as f32 / 1000.0;
        let axes: &[(&[u8; 4], f32)] = if tag == *b"wght" {
            &[(b"wght", value)]
        } else {
            &[(&other, value)]
        };
        text.measure(node, "a", &family, 24.0, options(&path, axes));
        for glyph in text.rasterize(node, (0.0, 0.0), 1.0, true) {
            keys.insert(glyph.cache_key);
        }
    }
    assert!(keys.len() > 8, "it did move: {} pictures", keys.len());
    let bound = if tag == *b"wght" { 801 } else { 129 };
    assert!(
        keys.len() <= bound,
        "{}: at most {bound} pictures, got {}",
        String::from_utf8_lossy(&tag),
        keys.len()
    );
}

#[test]
fn a_face_without_the_axis_is_drawn_as_it_always_was() {
    let mut scene = Scene::new();
    let plain = scene.create(Element::Text);
    let asked = scene.create(Element::Text);
    let mut text = TextSystem::new();
    text.measure(plain, "morf", "sans-serif", 20.0, TextOptions::default());
    let style = TextStyle {
        axes: vec![FontAxis {
            tag: *b"ZZZZ",
            value: 3.0,
        }],
        ..TextStyle::default()
    };
    text.measure(
        asked,
        "morf",
        "sans-serif",
        20.0,
        TextOptions {
            style,
            ..TextOptions::default()
        },
    );
    let keys = |text: &mut TextSystem, node| {
        text.rasterize(node, (0.0, 0.0), 1.0, true)
            .into_iter()
            .map(|glyph| glyph.cache_key)
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&mut text, plain), keys(&mut text, asked));
}
