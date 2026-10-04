use crate::{
    annotation::{Annotation, Kind, Point, draw},
    ops::{self, ImageOp, RegionEffect},
    preview::Preview,
};
use image::{DynamicImage, RgbaImage};
fn mark(kind: Kind) -> Annotation {
    Annotation {
        kind,
        points: vec![Point { x: 10.0, y: 10.0 }, Point { x: 40.0, y: 40.0 }],
        color: [255, 0, 0],
        width: 4.0,
        filled: true,
        text: "<test & text>".into(),
        font: "sans-serif".into(),
        number: 1,
    }
}
fn black() -> DynamicImage {
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(64, 64, image::Rgba([0, 0, 0, 255])))
}
#[test]
fn geometric_tools_draw_directly_and_keep_pixels_outside_their_tiles() {
    for kind in [
        Kind::Rect,
        Kind::Ellipse,
        Kind::Line,
        Kind::Arrow,
        Kind::Pen,
        Kind::Marker,
        Kind::Zoom,
        Kind::Step,
        Kind::Text,
    ] {
        let a = mark(kind);
        let (stroke, fill) = a.path_data();
        if kind != Kind::Text {
            assert!(!stroke.is_empty() || !fill.is_empty());
        }
        let image = draw(black(), &[a]).unwrap().into_rgba8();
        assert!(
            image.pixels().any(|p| p[0] > 0),
            "{kind:?} produced no mark"
        );
        assert_eq!(image.get_pixel(63, 63).0, [0, 0, 0, 255]);
        if matches!(
            kind,
            Kind::Rect | Kind::Ellipse | Kind::Line | Kind::Arrow | Kind::Pen | Kind::Marker
        ) {
            assert!(image.get_pixel(25, 25)[0] > 0, "{kind:?} missed its centre");
        }
    }
}
#[test]
fn ordered_redactions_affect_earlier_marks_and_later_marks_stay_crisp() {
    let mut red = mark(Kind::Rect);
    red.points[1] = Point { x: 30.0, y: 30.0 };
    let mut green = mark(Kind::Rect);
    green.color = [0, 255, 0];
    green.points = vec![Point { x: 20.0, y: 20.0 }, Point { x: 25.0, y: 25.0 }];
    let image = ops::apply_ops(
        black(),
        &[
            ImageOp::Annotations(vec![red]),
            ImageOp::Region {
                x: 5,
                y: 5,
                width: 40,
                height: 40,
                effect: RegionEffect::Blur(8.0),
            },
            ImageOp::Annotations(vec![green]),
        ],
    )
    .unwrap()
    .into_rgba8();
    assert!(image.get_pixel(12, 12)[0] < 255);
    assert_eq!(image.get_pixel(22, 22).0, [0, 255, 0, 255]);
}
#[test]
fn invalid_native_geometry_is_refused_before_rendering() {
    let mut a = mark(Kind::Pen);
    a.points.clear();
    assert!(draw(black(), &[a.clone()]).is_err());
    assert_eq!(a.path_data(), (String::new(), String::new()));
    a.points = vec![
        Point {
            x: f32::NAN,
            y: 0.0
        };
        2
    ];
    assert!(draw(black(), &[a]).is_err());
}

/// A repeatable comparison of the old PNG/SVG preview round trip and a
/// resident native preview. Run explicitly in release mode, outside CI.
#[test]
#[ignore]
fn benchmark_capture_preview() {
    use std::time::Instant;
    let dir = std::env::temp_dir().join(format!("morf-preview-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (width, height) in [(1920, 1080), (3840, 2160)] {
        let path = dir.join("base.png");
        let output = dir.join("preview.png");
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                // Repeatable varied pixels: a photograph would change compression cost.
                let value = ((x * 37 + y * 19) ^ (x * y)) as u8;
                pixels.extend_from_slice(&[
                    value,
                    value.wrapping_add(71),
                    value.wrapping_add(139),
                    255,
                ]);
            }
        }
        ops::save_rgba(width, height, pixels, &path, ops::OutputFormat::Png, 100).unwrap();
        let preview = Preview::new(path.clone());
        let a = mark(Kind::Rect);
        let native = [ImageOp::Annotations(vec![a])];
        preview.render(&native).unwrap(); // Warm the resident source; report steady edits.
        let mut old = Vec::new();
        let mut new = Vec::new();
        for i in 0..5 {
            let svg = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\"><rect x=\"10\" y=\"10\" width=\"30\" height=\"30\" fill=\"red\" stroke=\"red\" stroke-width=\"4\" stroke-linejoin=\"round\"/></svg>"
            );
            let start = Instant::now();
            ops::process(&ops::ProcessRequest {
                source: path.clone(),
                output: output.clone(),
                format: ops::OutputFormat::Png,
                quality: 100,
                ops: vec![ImageOp::Overlay {
                    source: svg.into(),
                    x: 0,
                    y: 0,
                }],
            })
            .unwrap();
            ops::decode_bounded(&output).unwrap();
            old.push(start.elapsed().as_secs_f64() * 1000.0);
            let start = Instant::now();
            let image = preview.render(&native).unwrap();
            let source = preview.publish(image).unwrap();
            let mut cache = crate::ImageCache::default();
            let pixels = cache.load(source, width, height, 120).unwrap();
            std::hint::black_box(pixels);
            new.push(start.elapsed().as_secs_f64() * 1000.0);
            std::hint::black_box(i);
        }
        old.sort_by(f64::total_cmp);
        new.sort_by(f64::total_cmp);
        println!(
            "{width}x{height}: PNG/SVG round trip {:.2} ms; resident typed preview {:.2} ms; {:.1}x speedup (median of five)",
            old[2],
            new[2],
            old[2] / new[2]
        );
        preview.close();
    }
    std::fs::remove_dir_all(dir).unwrap();
}
