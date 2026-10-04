use morf_layout::{Geometry, TextAlignment, TextElide, Transform2D};
use morf_scene::{Color, NodeHandle};

use crate::*;

use crate::gpu::field_tests::{alpha_at, read_frame, render_readback};

/// A text command, sized and styled, with everything else left alone.
pub(crate) fn text_command(
    node: NodeHandle,
    text: &str,
    size: f64,
    field_style: DistanceFieldStyle,
) -> DrawCommand {
    DrawCommand::Text {
        morph_to: String::new(),
        morph_progress: 0.0,
        style: morf_layout::TextStyle::default(),
        decoration: None,
        edit: None,
        node,
        bounds: Geometry {
            x: 0.0,
            y: 0.0,
            width: 256.0,
            height: 128.0,
        },
        transform: Transform2D::IDENTITY,
        clip: None,
        text: text.to_owned(),
        family: "sans-serif".to_owned(),
        font_source: String::new(),
        size,
        font_weight: 400.0,
        color: Color::rgba8(255, 255, 255, 255),
        color_overlay: Color::rgba8(0, 0, 0, 0),
        wrap: false,
        max_lines: 0,
        elide: TextElide::None,
        horizontal_alignment: TextAlignment::Left,
        vertical_alignment: VerticalAlignment::Top,
        field_style,
    }
}

/// How much ink a rendering has, as a fraction of the pixels it could cover.
pub(crate) fn ink(pixels: &[u8], size: u32) -> f64 {
    let covered = (0..size)
        .flat_map(|y| (0..size).map(move |x| (x, y)))
        .filter(|(x, y)| alpha_at(pixels, size, *x, *y) > 128)
        .count();
    covered as f64 / f64::from(size * size)
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_glyph_drawn_larger_covers_more_without_being_rasterized_again() {
    // The reason glyphs are stored as fields at all. One atlas entry, measured
    // once at a reference size, and the letter is drawn at whatever size is
    // asked for by scaling the quad. If this failed by drawing the same number
    // of pixels at both sizes, the field would be being sampled as if it were a
    // fixed-size bitmap.
    let mut scene = morf_scene::Scene::new();
    let node = scene.create(morf_scene::Element::Text);
    let render = |size| {
        let list = DrawList {
            commands: vec![text_command(node, "M", size, DistanceFieldStyle::default())],
            layers: Vec::new(),
        };
        ink(&render_readback(&list, 128), 128)
    };

    let small = render(24.0);
    let large = render(72.0);
    assert!(small > 0.0, "the small letter drew something: {small}");
    assert!(
        large > small * 3.0,
        "three times the size covers far more ground: {large} against {small}"
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn thickness_moves_the_edge_and_an_outline_adds_a_band_around_it() {
    // What a threshold buys that a coverage bitmap cannot: the edge is a number
    // rather than a set of pixels, so weight is that number moved and an
    // outline is a second one further out. Both are animatable, and neither
    // re-renders the glyph.
    let mut scene = morf_scene::Scene::new();
    let node = scene.create(morf_scene::Element::Text);
    let render = |style| {
        let list = DrawList {
            commands: vec![text_command(node, "M", 64.0, style)],
            layers: Vec::new(),
        };
        ink(&render_readback(&list, 128), 128)
    };

    let plain = render(DistanceFieldStyle::default());
    let bolder = render(DistanceFieldStyle {
        thickness: 2.0,
        ..DistanceFieldStyle::default()
    });
    let thinner = render(DistanceFieldStyle {
        thickness: -2.0,
        ..DistanceFieldStyle::default()
    });
    assert!(
        bolder > plain && plain > thinner,
        "the edge moved both ways: {thinner} < {plain} < {bolder}"
    );

    let outlined = render(DistanceFieldStyle {
        outline_width: 3.0,
        outline_color: Color::rgba8(255, 0, 0, 255),
        ..DistanceFieldStyle::default()
    });
    assert!(
        outlined > plain,
        "the outline is a band outside the fill: {outlined} against {plain}"
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_decoration_is_a_band_under_the_line() {
    // A line under the text is drawn from the face's own metrics: below the
    // baseline, as wide as the line, in the decoration's colour — and not
    // there at all when nothing asked for it.
    let mut scene = morf_scene::Scene::new();
    let node = scene.create(morf_scene::Element::Text);
    let render = |decoration: Option<morf_scene::TextDecoration>| {
        let mut command = text_command(node, "mmmm", 40.0, DistanceFieldStyle::default());
        let DrawCommand::Text {
            decoration: slot, ..
        } = &mut command
        else {
            panic!("text_command builds text");
        };
        *slot = decoration;
        render_readback(
            &DrawList {
                commands: vec![command],
                layers: Vec::new(),
            },
            128,
        )
    };
    let plain = render(None);
    let underlined = render(Some(morf_scene::TextDecoration {
        line: morf_scene::DecorationLine::Under,
        thickness: Some(4.0),
        offset: 0.0,
        color: Some(Color::rgba8(255, 0, 0, 255)),
    }));
    // A row below the letters' baseline that the underline runs along:
    // the widest run of red across any row.
    let red_run = |pixels: &[u8], y: u32| {
        (0..128u32)
            .filter(|x| {
                let i = ((y * 128 + x) * 4) as usize;
                pixels[i] > 200 && pixels[i + 1] < 60 && pixels[i + 3] > 200
            })
            .count()
    };
    let widest = (0..128u32).map(|y| red_run(&underlined, y)).max().unwrap();
    assert!(widest > 60, "the line runs the width of the text: {widest}");
    assert_eq!(
        (0..128u32).map(|y| red_run(&plain, y)).max().unwrap(),
        0,
        "no red without a decoration"
    );
    assert!(
        ink(&underlined, 128) > ink(&plain, 128),
        "the band adds ink"
    );
}

/// Where Material Symbols Rounded is installed, if it is and has a FILL axis.
fn icon_font() -> Option<std::path::PathBuf> {
    morf_text::family_files("Material Symbols Rounded")
        .into_iter()
        .find(|path| {
            morf_text::file_axes(path)
                .iter()
                .any(|axis| &axis.tag == b"FILL")
        })
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_fill_axis_fills_an_icon_on_screen() {
    // The icon font's FILL axis, through the whole path: the style carries
    // the axes, the text system draws the glyph's field at that point of the
    // design space, and the atlas holds the two as two pictures -- rendered
    // by one backend, one after the other, so a key that ignored the axes
    // would hand back the first picture for the second.
    let Some(path) = icon_font() else {
        eprintln!("no Material Symbols Rounded with a FILL axis here; skipped");
        return;
    };
    let mut scene = morf_scene::Scene::new();
    let node = scene.create(morf_scene::Element::Text);
    let command = |fill: f32| {
        let mut command = text_command(node, "favorite", 96.0, DistanceFieldStyle::default());
        if let DrawCommand::Text {
            family,
            font_source,
            style,
            ..
        } = &mut command
        {
            *family = "Material Symbols Rounded".to_owned();
            *font_source = path.to_string_lossy().into_owned();
            style.axes = vec![morf_layout::FontAxis {
                tag: *b"FILL",
                value: fill,
            }];
        }
        DrawList {
            commands: vec![command],
            layers: Vec::new(),
        }
    };
    let mut backend = pollster::block_on(WgpuBackend::new(128, 128)).unwrap();
    let mut draw = |fill| {
        ink(
            &crate::gpu::field_tests::read_frame(&mut backend, &command(fill), 128),
            128,
        )
    };
    let hollow = draw(0.0);
    let half = draw(0.5);
    let solid = draw(1.0);
    assert!(hollow > 0.01, "the outline drew: {hollow}");
    assert!(
        half > hollow * 1.2 && solid > half * 1.2,
        "filling in adds ink step by step: {hollow} {half} {solid}"
    );
}

/// Google Sans Flex, which has `wdth` and `opsz` axes that move advances.
fn flex_font() -> Option<std::path::PathBuf> {
    let store = "/nix/store/id27jgbl1sdj8mw04yrwx5bgsv4ap2xg-source/assets/google-sans-flex/GoogleSansFlex-VariableFont_GRAD,ROND,opsz,slnt,wdth,wght.ttf";
    std::env::var_os("MORF_TEST_VARIABLE_FONT")
        .map(std::path::PathBuf::from)
        .into_iter()
        .chain([std::path::PathBuf::from(store)])
        .chain(morf_text::family_files("Google Sans Flex"))
        .find(|path| {
            morf_text::file_axes(path)
                .iter()
                .any(|axis| &axis.tag == b"wdth")
        })
}

/// The rightmost column of a readback with any ink in it, plus one.
fn ink_right(pixels: &[u8], size: u32) -> u32 {
    (0..size)
        .rev()
        .find(|x| (0..size).any(|y| alpha_at(pixels, size, *x, y) > 32))
        .map_or(0, |x| x + 1)
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_wdth_axis_is_drawn_as_wide_as_it_measures() {
    // An axis that changes advances, through the whole path: the text is
    // shaped at the axis, laid out by that width, and drawn with its glyphs
    // where the shaper put them -- so the ink ends where the measured width
    // says the line does, narrow or wide.
    let Some(path) = flex_font() else {
        eprintln!("no Google Sans Flex here; skipped");
        return;
    };
    const SIZE: u32 = 256;
    let text = "nnnnnn";
    let mut scene = morf_scene::Scene::new();
    let node = scene.create(morf_scene::Element::Text);
    let style = |wdth: f32| morf_layout::TextStyle {
        axes: vec![morf_layout::FontAxis {
            tag: *b"wdth",
            value: wdth,
        }],
        ..morf_layout::TextStyle::default()
    };
    let command = |wdth: f32| {
        let mut command = text_command(node, text, 32.0, DistanceFieldStyle::default());
        if let DrawCommand::Text {
            family,
            font_source,
            style: drawn,
            ..
        } = &mut command
        {
            *family = "Google Sans Flex".to_owned();
            *font_source = path.to_string_lossy().into_owned();
            *drawn = style(wdth);
        }
        DrawList {
            commands: vec![command],
            layers: Vec::new(),
        }
    };
    let measured = |wdth: f32| {
        use morf_layout::TextMeasurer as _;
        let mut text_system = morf_text::TextSystem::new();
        text_system
            .measure(
                node,
                text,
                "Google Sans Flex",
                32.0,
                morf_layout::TextOptions {
                    font_source: Some(path.to_string_lossy().into_owned()),
                    style: style(wdth),
                    ..morf_layout::TextOptions::default()
                },
            )
            .width as f32
    };
    let mut backend = pollster::block_on(WgpuBackend::new(SIZE, SIZE)).unwrap();
    let mut right = |wdth| ink_right(&read_frame(&mut backend, &command(wdth), SIZE), SIZE) as f32;
    let (narrow, wide) = (right(50.0), right(151.0));
    let (narrow_width, wide_width) = (measured(50.0), measured(151.0));
    assert!(
        wide > narrow * 1.3,
        "wider is drawn wider: {narrow} against {wide}"
    );
    // The last `n` ends a right side bearing short of its advance: a pixel
    // or two at 32 px, never past it.
    for (ink, width) in [(narrow, narrow_width), (wide, wide_width)] {
        assert!(
            ink <= width + 1.0 && ink >= width - 5.0,
            "ink ends at {ink}, measured {width}"
        );
    }
}
