use std::sync::Arc;

use morf_layout::{TextMeasurer, TextOptions, TextStyle};
use morf_scene::{Element, RichText, Scene};

use crate::{SpanLine, TextSystem};

fn options(rich: RichText) -> TextOptions {
    TextOptions {
        style: TextStyle {
            rich: Some(Arc::new(rich)),
            link_color: morf_scene::Color::parse("#0000ff"),
            ..TextStyle::default()
        },
        ..TextOptions::default()
    }
}

#[test]
fn runs_are_shaped_with_their_own_style() {
    let mut scene = Scene::new();
    let (plain, rich) = (scene.create(Element::Text), scene.create(Element::Text));
    let mut text = TextSystem::new();
    let markup =
        RichText::from_markup("see <b>this</b> <a href=\"https://morf\">link</a> <s>gone</s>");
    let plain_size = text.measure(
        plain,
        &markup.text,
        "sans-serif",
        16.0,
        TextOptions::default(),
    );
    let rich_size = text.measure(rich, "ignored", "sans-serif", 16.0, options(markup));
    // Bold is wider than regular, so the same letters take more room.
    assert!(
        rich_size.width > plain_size.width,
        "{rich_size:?} vs {plain_size:?}"
    );
    // The link is where its letters are, and nowhere else.
    let links = text.link_rects(rich);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].href, "https://morf");
    assert!(links[0].x > 30.0 && links[0].width > 10.0, "{links:?}");
    assert!(links[0].x + links[0].width < rich_size.width as f32);
    // Underlined because it is a link; struck because it asked.
    let bands = text.span_bands(rich);
    assert_eq!(bands.len(), 2, "{bands:?}");
    assert_eq!(bands[0].line, SpanLine::Under);
    assert_eq!(bands[0].tint, Some([0, 0, 255, 255]));
    assert_eq!(bands[1].line, SpanLine::Through);
    assert!(bands[1].band.x > bands[0].band.x);
    // The link's glyphs carry its colour; the rest carry none.
    let glyphs = text.rasterize(rich, (0.0, 0.0), 1.0, true);
    let blue = glyphs
        .iter()
        .filter(|glyph| glyph.tint == Some([0, 0, 255, 255]))
        .count();
    assert_eq!(blue, 4, "the four letters of `link`");
    assert!(glyphs.iter().filter(|glyph| glyph.tint.is_none()).count() > 8);
    assert!(text.link_rects(plain).is_empty() && text.span_bands(plain).is_empty());
}

#[test]
fn a_run_may_set_its_own_size_and_an_elided_run_keeps_its_style() {
    let mut scene = Scene::new();
    let node = scene.create(Element::Text);
    let mut text = TextSystem::new();
    let big = RichText::from_spans(&morf_scene::Value::List(vec![
        morf_scene::Value::String("small ".to_owned()),
        morf_scene::Value::Map(std::collections::BTreeMap::from([
            (
                "text".to_owned(),
                morf_scene::Value::String("BIG".to_owned()),
            ),
            ("size".to_owned(), morf_scene::Value::Number(40.0)),
        ])),
    ]))
    .unwrap()
    .unwrap();
    let size = text.measure(node, "", "sans-serif", 12.0, options(big));
    assert!(size.height > 40.0, "the line grew to the big run: {size:?}");
    let glyphs = text.rasterize(node, (0.0, 0.0), 1.0, true);
    assert!(glyphs.iter().any(|glyph| glyph.font_size == 40.0));
    assert!(glyphs.iter().any(|glyph| glyph.font_size == 12.0));

    // Elided to a width, the kept head keeps its runs.
    let long = RichText::from_markup("<b>bold words</b> then a great deal more plain text after");
    let elided = text.measure(
        node,
        "",
        "sans-serif",
        16.0,
        TextOptions {
            width: Some(120.0),
            elide: morf_layout::TextElide::Right,
            ..options(long.clone())
        },
    );
    assert!(elided.width <= 125.0, "{elided:?}");
    let segments = crate::rich::segments(&long, "bold words the…");
    assert_eq!(segments[0], (0..10, 0));
    assert_eq!(segments.last().unwrap().0.end, "bold words the…".len());
}
