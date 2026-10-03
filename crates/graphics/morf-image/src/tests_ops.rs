use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use image::{Rgba, RgbaImage};

use crate::ops::*;
use crate::*;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[test]
fn native_canvas_preserves_alpha_orders_overlays_and_refuses_oversize_before_writing() {
    let root = temp_dir("canvas");
    let mut request = crate::canvas::Request {
        width: 40,
        height: 20,
        background: [12, 34, 56, 128],
        ops: vec![],
        output: root.join("canvas.png"),
        format: OutputFormat::Png,
        quality: 90,
    };
    crate::canvas::compose(&request).unwrap();
    assert_eq!(
        pixel_at(&request.output, 0, 0, 800).unwrap(),
        request.background
    );
    let source = root.join("source.png");
    two_tone(&source);
    request.ops = vec![
        ImageOp::Overlay { source, x: 0, y: 0 },
        ImageOp::Crop {
            x: 20,
            y: 0,
            width: 20,
            height: 10,
        },
    ];
    let info = crate::canvas::compose(&request).unwrap();
    assert_eq!((info.width, info.height), (20, 10));
    assert_eq!(
        pixel_at(&request.output, 10, 5, 200).unwrap(),
        [0, 255, 0, 255]
    );
    request.width = MAX_DIMENSION + 1;
    request.output = root.join("refused.png");
    assert!(crate::canvas::compose(&request).is_err());
    assert!(!request.output.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn annotation_compositing_and_region_effects_preserve_outside_pixels() {
    let base = image::DynamicImage::ImageRgba8(RgbaImage::from_fn(40, 20, |x, y| {
        if (x + y) % 2 == 0 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    }));
    for effect in [
        RegionEffect::Blur(4.0),
        RegionEffect::Pixelate(4),
        RegionEffect::Zoom(2.0),
    ] {
        let edited = apply_ops(
            base.clone(),
            &[ImageOp::Region {
                x: 10,
                y: 5,
                width: 20,
                height: 10,
                effect,
            }],
        )
        .unwrap()
        .to_rgba8();
        assert_eq!(edited.get_pixel(0, 0), base.to_rgba8().get_pixel(0, 0));
        assert_eq!(edited.get_pixel(39, 19), base.to_rgba8().get_pixel(39, 19));
        assert_ne!(edited.get_pixel(15, 8), base.to_rgba8().get_pixel(15, 8));
    }
    let edited=apply_ops(base,&[
        ImageOp::Overlay {source:PathBuf::from(r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect x="10" y="5" width="20" height="10" fill="#00ff00"/></svg>"##),x:0,y:0},
        ImageOp::Crop {x:10,y:5,width:20,height:10},
    ]).unwrap().to_rgba8();
    assert_eq!(edited.dimensions(), (20, 10));
    assert_eq!(*edited.get_pixel(5, 5), Rgba([0, 255, 0, 255]));
}

#[test]
fn svg_annotation_text_is_drawn_and_invalid_effects_are_refused() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="40"><text x="2" y="30" font-size="24" font-family="sans-serif" fill="red">Test</text></svg>"##;
    let decoded = decode_bounded(svg).unwrap().to_rgba8();
    assert!(decoded.pixels().any(|p| p[3] > 0 && p[0] > 200));
    for effect in [
        RegionEffect::Blur(f32::NAN),
        RegionEffect::Pixelate(0),
        RegionEffect::Zoom(100.0),
    ] {
        assert!(
            apply_ops(
                image::DynamicImage::ImageRgba8(decoded.clone()),
                &[ImageOp::Region {
                    x: 0,
                    y: 0,
                    width: 20,
                    height: 20,
                    effect
                }]
            )
            .is_err()
        );
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let path = env::temp_dir().join(format!("morf-image-ops-{name}-{}-{id}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

/// A 40x20 picture: left half red, right half blue, one green pixel at 30,5.
fn two_tone(path: &std::path::Path) {
    let mut image = RgbaImage::from_fn(40, 20, |x, _| {
        if x < 20 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    });
    image.put_pixel(30, 5, Rgba([0, 255, 0, 255]));
    image.save(path).unwrap();
}

#[test]
fn info_reads_the_header() {
    let root = temp_dir("info");
    let path = root.join("a.png");
    two_tone(&path);
    let info = image_info(&path).unwrap();
    assert_eq!(
        (info.width, info.height, info.format.as_str()),
        (40, 20, "png")
    );
    assert!(image_info(root.join("missing.png")).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn process_runs_operations_in_order_and_writes_the_format_asked() {
    let root = temp_dir("process");
    let source = root.join("a.png");
    two_tone(&source);

    let square = ProcessRequest {
        source: source.clone(),
        ops: vec![
            ImageOp::Square,
            ImageOp::Resize {
                width: 10,
                height: 0,
                mode: ResizeMode::Fit,
            },
        ],
        output: root.join("square.png"),
        format: OutputFormat::Png,
        quality: 90,
    };
    let info = process(&square).unwrap();
    assert_eq!((info.width, info.height), (10, 10));
    // The centre square of a 40x20 picture is x 10..30: half red, half blue.
    let written = image::open(root.join("square.png")).unwrap().to_rgba8();
    assert_eq!(written.get_pixel(0, 5).0, [255, 0, 0, 255]);
    assert_eq!(written.get_pixel(9, 5).0, [0, 0, 255, 255]);

    let edit = ProcessRequest {
        source: source.clone(),
        ops: vec![
            ImageOp::Crop {
                x: 20,
                y: 0,
                width: 100,
                height: 10,
            },
            ImageOp::Rotate(90),
            ImageOp::Flip { horizontal: false },
            ImageOp::Grayscale,
            ImageOp::Blur(0.5),
        ],
        output: root.join("edit.jpg"),
        format: OutputFormat::from_path(&root.join("edit.jpg")).unwrap(),
        quality: 80,
    };
    let info = process(&edit).unwrap();
    // Cropped to 20x10, then turned: 10 wide and 20 tall.
    assert_eq!(
        (info.width, info.height, info.format.as_str()),
        (10, 20, "jpeg")
    );
    assert_eq!(image_info(root.join("edit.jpg")).unwrap().format, "jpeg");

    let fill = ProcessRequest {
        ops: vec![ImageOp::Resize {
            width: 8,
            height: 8,
            mode: ResizeMode::Fill,
        }],
        output: root.join("fill.webp"),
        format: OutputFormat::Webp,
        ..square.clone()
    };
    let info = process(&fill).unwrap();
    assert_eq!((info.width, info.height), (8, 8));
    assert_eq!(image_info(root.join("fill.webp")).unwrap().format, "webp");

    let missed = ProcessRequest {
        ops: vec![ImageOp::Crop {
            x: 50,
            y: 0,
            width: 5,
            height: 5,
        }],
        output: root.join("missed.png"),
        ..square.clone()
    };
    assert!(process(&missed).is_err());
    assert!(!root.join("missed.png").exists());
    // No temporary file is left beside a failed write.
    assert_eq!(
        fs::read_dir(&root)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp")
            })
            .count(),
        0
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn oversized_pictures_are_refused_from_the_header() {
    let root = temp_dir("huge");
    let path = root.join("huge.png");
    // A real header claiming 20000x10: refused before any pixel is decoded.
    RgbaImage::new(20_000, 1).save(&path).unwrap();
    let error = decode_bounded(&path).unwrap_err();
    assert!(error.to_string().contains("limit"), "{error}");
    assert!(check_size(16_384, 4_096).is_ok());
    assert!(check_size(16_384, 16_384).is_err(), "over 256 MiB decoded");
    let request = ProcessRequest {
        source: root.join("small.png"),
        ops: vec![ImageOp::Resize {
            width: 20_000,
            height: 0,
            mode: ResizeMode::Exact,
        }],
        output: root.join("out.png"),
        format: OutputFormat::Png,
        quality: 90,
    };
    two_tone(&request.source);
    assert!(process(&request).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn pixel_and_palette_read_what_is_there() {
    let root = temp_dir("pixel");
    let path = root.join("a.png");
    two_tone(&path);
    assert_eq!(pixel_at(&path, 30, 5, u64::MAX).unwrap(), [0, 255, 0, 255]);
    assert_eq!(pixel_at(&path, 0, 0, u64::MAX).unwrap(), [255, 0, 0, 255]);
    assert!(pixel_at(&path, 40, 0, u64::MAX).is_err());
    assert!(pixel_at(&path, 0, 0, 100).is_err(), "bounded by the caller");

    // Three quarters blue: the order and the shares say so.
    let mut image = RgbaImage::from_pixel(40, 40, Rgba([0, 0, 255, 255]));
    for y in 0..40 {
        for x in 0..10 {
            image.put_pixel(x, y, Rgba([250, 250, 0, 255]));
        }
    }
    let skewed = root.join("skewed.png");
    image.save(&skewed).unwrap();
    let colours = palette(&skewed, 2).unwrap();
    assert_eq!(colours.len(), 2);
    assert_eq!(colours[0].rgba, [0, 0, 255, 255]);
    assert!((colours[0].fraction - 0.75).abs() < 0.01, "{colours:?}");
    assert_eq!(colours[1].rgba, [250, 250, 0, 255]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn raw_pixels_save_to_a_file() {
    let root = temp_dir("save");
    let path = root.join("capture.png");
    let rgba = [10, 20, 30, 255].repeat(6);
    let info = save_rgba(3, 2, rgba, &path, OutputFormat::Png, 90).unwrap();
    assert_eq!((info.width, info.height), (3, 2));
    let back = image::open(&path).unwrap().to_rgba8();
    assert_eq!(back.get_pixel(2, 1).0, [10, 20, 30, 255]);
    assert!(save_rgba(3, 2, vec![0; 5], &path, OutputFormat::Png, 90).is_err());
    fs::remove_dir_all(root).unwrap();
}

const SQUARE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="2"><rect width="4" height="2" fill="#00ff00"/></svg>"##;

#[test]
fn inline_svg_and_data_uris_are_sources() {
    let mut cache = ImageCache::default();
    let image = cache.load(SQUARE, 4, 2, 120).unwrap();
    assert_eq!((image.width, image.height), (4, 2));
    assert_eq!(&image.rgba[..4], &[0, 255, 0, 255]);
    assert_eq!(cache.intrinsic_size(SQUARE).unwrap(), (4, 2));

    let escaped = format!("data:image/svg+xml,{}", SQUARE.replace('#', "%23"));
    assert_eq!(cache.intrinsic_size(&escaped).unwrap(), (4, 2));
    use base64::Engine as _;
    let encoded = base64::engine::general_purpose::STANDARD.encode(SQUARE);
    let base64_uri = format!("data:image/svg+xml;base64,{encoded}");
    let image = cache.load(&base64_uri, 4, 2, 120).unwrap();
    assert_eq!(&image.rgba[..4], &[0, 255, 0, 255]);

    let mut png = Vec::new();
    RgbaImage::from_pixel(3, 3, Rgba([9, 8, 7, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let png_uri = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&png)
    );
    assert_eq!(cache.intrinsic_size(&png_uri).unwrap(), (3, 3));
    assert_eq!(
        &cache.load(&png_uri, 3, 3, 120).unwrap().rgba[..4],
        &[9, 8, 7, 255]
    );
    assert_eq!(pixel_at(&png_uri, 1, 1, 100).unwrap(), [9, 8, 7, 255]);
    assert_eq!(image_info(&png_uri).unwrap().format, "png");
    assert_eq!(image_info(SQUARE).unwrap().format, "svg");

    assert!(is_inline_source("  <svg/>") && is_inline_source("DATA:image/png;base64,"));
    assert!(!is_inline_source("/tmp/a.svg") && !is_inline_source("memory:capture/1"));
    assert!(cache.load("data:text/plain,hello", 4, 4, 120).is_err());
    assert!(cache.load("data:image/png;base64,@@@", 4, 4, 120).is_err());
}

#[test]
fn large_preview_cache_is_bounded_by_bytes_before_the_count_limit() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="red"/></svg>"##;
    let mut cache = ImageCache::default();
    let first = cache.load(source, 2048, 2048, 120).unwrap();
    for size in 2049..2053 {
        cache.load(source, size, 2048, 120).unwrap();
    }
    cache.shrink();
    let next = cache.load(source, 2048, 2048, 120).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&first, &next));
}
